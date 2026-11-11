# A stand-in of the egress socket of a unit (T-065): an HTTP CONNECT proxy
# on a unix socket. It writes each CONNECT target to LOG, one on each line,
# and tunnels each one to the loopback port PORT, whatever the target.
#
#   python3 -I scripts/connect-stand-in.py SOCKET LOG PORT
import os, socket, sys, threading

path, log, port = sys.argv[1], sys.argv[2], int(sys.argv[3])


def pipe(a, b):
    try:
        while True:
            data = a.recv(65536)
            if not data:
                break
            b.sendall(data)
    except OSError:
        pass
    for s in (a, b):
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass


def serve(c):
    head = b""
    while b"\r\n\r\n" not in head and len(head) < 8192:
        part = c.recv(1024)
        if not part:
            c.close()
            return
        head += part
    line = head.split(b"\r\n", 1)[0].decode("latin-1")
    with open(log, "a") as f:
        f.write(line + "\n")
    if not line.startswith("CONNECT "):
        c.sendall(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n")
        c.close()
        return
    up = socket.create_connection(("127.0.0.1", port))
    c.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
    rest = head.split(b"\r\n\r\n", 1)[1]
    if rest:
        up.sendall(rest)
    threading.Thread(target=pipe, args=(c, up), daemon=True).start()
    pipe(up, c)


if os.path.exists(path):
    os.unlink(path)
srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
srv.bind(path)
srv.listen(16)
while True:
    conn, _ = srv.accept()
    threading.Thread(target=serve, args=(conn,), daemon=True).start()
