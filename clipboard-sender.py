import os, socket, struct, sys, subprocess

IP = os.environ.get('CLIP_SYNC_IP', '172.16.34.128')
PORT = 9999

# Preference order for MIME types when multiple are available
MIME_PRIORITY = [
    'image/png', 'image/jpeg', 'image/gif', 'image/bmp', 'image/webp',
    'text/plain;charset=utf-8', 'text/plain', 'UTF8_STRING', 'STRING',
]

def recvall(s, n):
    buf = b''
    while len(buf) < n:
        chunk = s.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("Connection closed before all data received")
        buf += chunk
    return buf

def get_clipboard():
    result = subprocess.run(['wl-paste', '--list-types'], capture_output=True, text=True)
    available = [t.strip() for t in result.stdout.splitlines() if t.strip()]
    if not available:
        raise RuntimeError("Clipboard is empty or wl-paste failed")

    # Pick best available type by priority list
    mime = next((p for p in MIME_PRIORITY if p in available), available[0])

    data = subprocess.run(['wl-paste', '--no-newline', '--type', mime],
                          capture_output=True).stdout

    # Normalize charset variants so Windows side sees plain text/plain
    if mime.startswith('text/plain'):
        mime = 'text/plain'

    return mime, data

def set_clipboard(mime, data):
    subprocess.run(['wl-copy', '--type', mime], input=data)

def push():
    mime, data = get_clipboard()
    mime_b = mime.encode('utf-8')
    payload = struct.pack('<ii', 1, len(mime_b)) + mime_b + struct.pack('<i', len(data)) + data
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((IP, PORT))
        s.sendall(payload)
    print(f"Pushed {mime} ({len(data)} bytes) to Windows.")

def pull():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((IP, PORT))
        s.sendall(struct.pack('<i', 2))
        # Response: [status(4)][mimeLen(4)][mimeBytes][dataLen(4)][data]
        status, m_len = struct.unpack('<ii', recvall(s, 8))
        mime = recvall(s, m_len).decode('utf-8')
        d_len = struct.unpack('<i', recvall(s, 4))[0]
        data = recvall(s, d_len)
    set_clipboard(mime, data)
    print(f"Pulled {mime} ({d_len} bytes) from Windows into clipboard.")

if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else None
    if cmd == "push":
        push()
    elif cmd == "pull":
        pull()
    else:
        print("Usage: clipboard-sender.py push|pull")
        sys.exit(1)
