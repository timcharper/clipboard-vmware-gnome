Add-Type -AssemblyName System.Windows.Forms, System.Drawing
if (-not ("Win32Clipboard" -as [type])) {
    Add-Type -TypeDefinition @"
using System.Runtime.InteropServices;
public class Win32Clipboard {
    [DllImport("user32.dll")]
    public static extern uint GetClipboardSequenceNumber();
}
"@ -Language CSharp
}

$port = 9999
$listener = [System.Net.Sockets.TcpListener]($port)
try {
    $listener.Start()
} catch {
    Write-Error "Failed to start on port ${port}: $($_.Exception.Message)"
    Write-Error "Is another instance already running? Try: Stop-Process -Name pwsh -Force"
    exit 1
}
Write-Host "clip-sync guest bridge active on port $port... (Ctrl+C to stop)" -ForegroundColor Cyan

while($true) {
    # Poll so Ctrl+C can interrupt instead of blocking forever on AcceptTcpClient
    while (-not $listener.Pending()) {
        Start-Sleep -Milliseconds 100
    }

    $client = $null
    try {
        $client = $listener.AcceptTcpClient()
        $stream = $client.GetStream()
        $br = New-Object System.IO.BinaryReader($stream)
        $bw = New-Object System.IO.BinaryWriter($stream)
        $command = $br.ReadInt32()

        if ($command -eq 1) {
            # --- PUSH: set clipboard (Linux -> Windows) ---
            $mimeLen = $br.ReadInt32()
            $mimeType = [System.Text.Encoding]::UTF8.GetString($br.ReadBytes($mimeLen))
            $dataLen = $br.ReadInt32()
            $payload = $br.ReadBytes($dataLen)

            if ($mimeType -eq "text/plain") {
                [System.Windows.Forms.Clipboard]::SetText([System.Text.Encoding]::UTF8.GetString($payload))
            } elseif ($mimeType -match "image") {
                $ms = New-Object System.IO.MemoryStream(,$payload)
                [System.Windows.Forms.Clipboard]::SetImage([System.Drawing.Image]::FromStream($ms))
            }
            Write-Host "Set Windows clipboard ($mimeType, $($payload.Length) bytes)" -ForegroundColor Green
        }
        elseif ($command -eq 2) {
            # --- PULL: send clipboard to Linux (Windows -> Linux) ---
            if ([System.Windows.Forms.Clipboard]::ContainsImage()) {
                $img = [System.Windows.Forms.Clipboard]::GetImage()
                $ms = New-Object System.IO.MemoryStream
                $img.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
                $data = $ms.ToArray()
                $mime = "image/png"
            } else {
                $data = [System.Text.Encoding]::UTF8.GetBytes([System.Windows.Forms.Clipboard]::GetText())
                $mime = "text/plain"
            }

            $mimeBytes = [System.Text.Encoding]::UTF8.GetBytes($mime)
            $bw.Write([int]1) # success code
            $bw.Write([int]$mimeBytes.Length)
            $bw.Write($mimeBytes)
            $bw.Write([int]$data.Length)
            $bw.Write($data)
            Write-Host "Sent Windows clipboard to Linux ($mime, $($data.Length) bytes)" -ForegroundColor Yellow
        }
        elseif ($command -eq 3) {
            # --- GET SEQUENCE NUMBER ---
            $seq = [Win32Clipboard]::GetClipboardSequenceNumber()
            $bw.Write([uint32]$seq)
            Write-Host "Sent sequence number: $seq" -ForegroundColor Magenta
        }
    }
    catch { Write-Warning "Connection error: $($_.Exception.Message)" }
    finally { if ($null -ne $client) { $client.Close() } }
}
