
import fcntl, os, pty, select, signal, sys, termios, time, tty
delay = float(sys.argv[1])
size = fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8)
pid, fd = pty.fork()
if pid == 0:
    fcntl.ioctl(0, termios.TIOCSWINSZ, size)
    os.execvp(sys.argv[2], sys.argv[2:])
def winch(*_args):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8))
signal.signal(signal.SIGWINCH, winch)
tty.setraw(0)
queue = []
while True:
    timeout = max(0.0, queue[0][0] - time.monotonic()) if queue else None
    ready = select.select([0, fd], [], [], timeout)[0]
    if 0 in ready:
        data = os.read(0, 4096)
        if not data:
            break
        os.write(fd, data)
    if fd in ready:
        try:
            data = os.read(fd, 4096)
        except OSError:
            data = b""
        if not data:
            break
        queue.append((time.monotonic() + delay, data))
    while queue and queue[0][0] <= time.monotonic():
        os.write(1, queue.pop(0)[1])
for _due, data in queue:
    os.write(1, data)
