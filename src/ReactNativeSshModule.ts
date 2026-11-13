import { NativeModule, requireNativeModule } from 'expo';

import type {
  GeneratedKey,
  NativeConnectConfig,
  NativeExecOutput,
  NativeShellOptions,
  ParsedKey,
  SshEvents,
} from './ReactNativeSsh.types';

declare class ReactNativeSshModule extends NativeModule<SshEvents> {
  coreVersion(): string;
  generateEd25519Key(comment: string): Promise<GeneratedKey>;
  parsePrivateKey(pem: string, passphrase: string | null): ParsedKey;
  connect(config: NativeConnectConfig): Promise<string>;
  exec(sessionId: string, command: string): Promise<NativeExecOutput>;
  disconnect(sessionId: string): Promise<void>;
  /** Returns the id of a shell whose bytes go to a native `SshShellSource`. */
  openShell(sessionId: string, options: NativeShellOptions): Promise<string>;
  closeShell(shellId: string): void;
  configureBackgroundSessions(
    channelName: string,
    title: string,
    disconnectAllLabel: string
  ): number;
  updateSessionsBody(body: string): void;
  notifyWhenIdle(shellId: string, quietMs: number, title: string, body: string | null): boolean;
  answerHostKey(requestId: string, accept: boolean): boolean;
  answerPrompts(requestId: string, answers: string[] | null): boolean;
  isRetryableCode(code: string): boolean;
}

export default requireNativeModule<ReactNativeSshModule>('ReactNativeSsh');
