Add-Type -AssemblyName System.Windows.Forms, System.Drawing

$port = 9999
$listener = [System.Net.Sockets.TcpListener]($port)
$listener.Start()
Write-Host "Affinity Bidirectional Bridge Active on $port... (Ctrl+C to stop)" -ForegroundColor Cyan

while($true) {
    # Poll so Ctrl+C can interrupt instead of blocking forever on AcceptTcpClient
    while (-not $listener.Pending()) {
        Start-Sleep -Milliseconds 100
    }
    $client = $listener.AcceptTcpClient()
    $stream = $client.GetStream()
    $br = New-Object System.IO.BinaryReader($stream)
    $bw = New-Object System.IO.BinaryWriter($stream)

    try {
        $command = $br.ReadInt32()

        if ($command -eq 1) { 
            # --- SET CLIPBOARD (Linux -> Windows) ---
            $mimeLen = $br.ReadInt32(); $mimeType = [System.Text.Encoding]::UTF8.GetString($br.ReadBytes($mimeLen))
            $dataLen = $br.ReadInt32(); $payload = $br.ReadBytes($dataLen)
            
            if ($mimeType -eq "text/plain") {
                [System.Windows.Forms.Clipboard]::SetText([System.Text.Encoding]::UTF8.GetString($payload))
            } elseif ($mimeType -match "image") {
                $ms = New-Object System.IO.MemoryStream(,$payload)
                [System.Windows.Forms.Clipboard]::SetImage([System.Drawing.Image]::FromStream($ms))
            }
            Write-Host "Set Windows Clipboard ($mimeType)" -ForegroundColor Green
        } 
        elseif ($command -eq 2) { 
            # --- GET CLIPBOARD (Windows -> Linux) ---
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

            # Send back the response in the same format
            $mimeBytes = [System.Text.Encoding]::UTF8.GetBytes($mime)
            $bw.Write([int]1) # Success code
            $bw.Write([int]$mimeBytes.Length)
            $bw.Write($mimeBytes)
            $bw.Write([int]$data.Length)
            $bw.Write($data)
            Write-Host "Sent Clipboard to Linux ($mime)" -ForegroundColor Yellow
        }
    }
    catch { Write-Warning "Error: $($_.Exception.Message)" }
    finally { $client.Close() }
}