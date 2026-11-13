import { WebSocket, WebSocketServer } from 'ws';
import * as pty from 'node-pty';

const PORT = Number(process.env.PORT ?? 7681);
const DEFAULT_COLS = 80;
const DEFAULT_ROWS = 24;

const server = new WebSocketServer({ port: PORT });

server.on('connection', (socket, request) => {
  const query = new URL(request.url ?? '/', 'ws://localhost').searchParams;
  const term = pty.spawn(process.env.SHELL ?? 'bash', ['-l'], {
    name: 'xterm-256color',
    cols: Number(query.get('cols') ?? DEFAULT_COLS),
    rows: Number(query.get('rows') ?? DEFAULT_ROWS),
    cwd: process.env.HOME,
    env: { ...process.env, TERM: 'xterm-256color' },
  });

  const sendOutput = (chunk) => {
    if (socket.readyState === WebSocket.OPEN) socket.send(Buffer.from(chunk, 'utf8'));
  };

  term.onData(sendOutput);
  term.onExit(({ exitCode }) => {
    sendOutput(`\r\n[pty exited with ${exitCode}]\r\n`);
    socket.close();
  });

  socket.on('message', (data, isBinary) => {
    if (isBinary) {
      term.write(Buffer.from(data).toString('utf8'));
      return;
    }

    try {
      const message = JSON.parse(Buffer.from(data).toString('utf8'));
      if (message.type === 'resize') term.resize(Number(message.cols), Number(message.rows));
    } catch {
      term.write(Buffer.from(data).toString('utf8'));
    }
  });

  socket.on('close', () => term.kill());
});

process.on('SIGINT', () => {
  for (const socket of server.clients) socket.close();
  server.close(() => process.exit(0));
});

console.log(`pty server listening on ws://0.0.0.0:${PORT}`);
