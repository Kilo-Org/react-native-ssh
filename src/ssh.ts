import type { EventSubscription } from 'expo';

import {
  SSH_ERROR_CODES,
  type ConnectOptions,
  type ExecOutput,
  type NativeAuth,
  type ParsedKey,
  type ShellOptions,
  type SshErrorCode,
  type SshEvents,
} from './ReactNativeSsh.types';
import ReactNativeSsh from './ReactNativeSshModule';

const DEFAULT_PORT = 22;
const DEFAULT_CONNECT_TIMEOUT_MS = 15_000;
const DEFAULT_KEEPALIVE_INTERVAL_S = 15;
const DEFAULT_TERM = 'xterm-256color';
const DEFAULT_COLS = 80;
const DEFAULT_ROWS = 24;

/** A failed SSH call, categorised by [`SshErrorCode`]. */
export class SshError extends Error {
  readonly code: SshErrorCode;

  constructor(code: SshErrorCode, message: string) {
    super(message);
    this.name = 'SshError';
    this.code = code;
  }
}

/**
 * Whether repeating the call that failed can succeed without changing its
 * inputs. Only transient transport failures are retryable.
 */
export function isRetryableError(error: SshError): boolean {
  return ReactNativeSsh.isRetryableCode(error.code);
}

/**
 * Connects and authenticates. Host-key and keyboard-interactive decisions
 * arrive through [`addSshListener`] while this promise is pending.
 */
export function connect(options: ConnectOptions): Promise<string> {
  return guard(() =>
    ReactNativeSsh.connect({
      host: options.host,
      port: options.port ?? DEFAULT_PORT,
      username: options.username,
      auth: nativeAuth(options.auth),
      connectTimeoutMs: options.connectTimeoutMs ?? DEFAULT_CONNECT_TIMEOUT_MS,
      keepaliveIntervalS: options.keepaliveIntervalS ?? DEFAULT_KEEPALIVE_INTERVAL_S,
    })
  );
}

/** Runs one command against a fresh session channel. */
export async function exec(sessionId: string, command: string): Promise<ExecOutput> {
  const output = await guard(() => ReactNativeSsh.exec(sessionId, command));

  return {
    stdout: output.stdout,
    stderr: output.stderr,
    exitStatus: output.exitStatus,
  };
}

/** Ends the session. Shell channels opened on it close as well. */
export function disconnect(sessionId: string): Promise<void> {
  return guard(() => ReactNativeSsh.disconnect(sessionId));
}

/**
 * Opens a pty-backed shell. The returned id names a native byte source for the
 * terminal module; its bytes never cross into JavaScript.
 */
export function openShell(sessionId: string, options: ShellOptions = {}): Promise<string> {
  return guard(() =>
    ReactNativeSsh.openShell(sessionId, {
      term: options.term ?? DEFAULT_TERM,
      cols: options.cols ?? DEFAULT_COLS,
      rows: options.rows ?? DEFAULT_ROWS,
    })
  );
}

export function closeShell(shellId: string): void {
  ReactNativeSsh.closeShell(shellId);
}

/** Reads an OpenSSH or PEM private key without contacting a server. */
export function parsePrivateKey(pem: string, passphrase?: string): ParsedKey {
  try {
    return ReactNativeSsh.parsePrivateKey(pem, passphrase ?? null);
  } catch (cause) {
    throw translate(cause);
  }
}

/** Accepts or rejects the host key of a pending connection. */
export function answerHostKey(requestId: string, accept: boolean): boolean {
  return ReactNativeSsh.answerHostKey(requestId, accept);
}

/** Answers a keyboard-interactive round; `null` cancels authentication. */
export function answerPrompts(requestId: string, answers: string[] | null): boolean {
  return ReactNativeSsh.answerPrompts(requestId, answers);
}

export function addSshListener<K extends keyof SshEvents>(
  event: K,
  listener: SshEvents[K]
): EventSubscription {
  return ReactNativeSsh.addListener(event, listener);
}

export type {
  ConnectOptions,
  DisconnectNotice,
  ExecOutput,
  HostKeyRequest,
  KeyboardInteractivePrompt,
  KeyboardInteractiveRequest,
  ParsedKey,
  ShellOptions,
  SshErrorCode,
  SshEvents,
} from './ReactNativeSsh.types';

function nativeAuth(auth: ConnectOptions['auth']): NativeAuth {
  switch (auth.type) {
    case 'password':
      return { type: 'password', password: auth.password };
    case 'key':
      return {
        type: 'key',
        privateKeyOpenssh: auth.privateKeyOpenssh,
        passphrase: auth.passphrase ?? null,
      };
    case 'keyboardInteractive':
      return { type: 'keyboardInteractive' };
  }
}

async function guard<T>(call: () => Promise<T>): Promise<T> {
  try {
    return await call();
  } catch (cause) {
    throw translate(cause);
  }
}

function translate(cause: unknown): SshError {
  if (cause instanceof SshError) {
    return cause;
  }

  const message = cause instanceof Error ? cause.message : String(cause);
  return new SshError(codeOf(cause), message);
}

function codeOf(cause: unknown): SshErrorCode {
  const candidate = hasCode(cause) ? cause.code : undefined;
  const match = SSH_ERROR_CODES.find((code) => code === candidate);

  return match ?? 'Protocol';
}

function hasCode(cause: unknown): cause is { code: unknown } {
  return typeof cause === 'object' && cause !== null && 'code' in cause;
}
