import { NativeModule, registerWebModule } from 'expo';

import type {
  GeneratedKey,
  NativeConnectConfig,
  NativeExecOutput,
  NativeShellOptions,
  ParsedKey,
  SshEvents,
} from './ReactNativeSsh.types';

const unavailable = '@kilocode/react-native-ssh has no web implementation';

function unsupported(): never {
  throw new Error(unavailable);
}

class ReactNativeSshModule extends NativeModule<SshEvents> {
  coreVersion(): string {
    throw new Error(unavailable);
  }

  generateEd25519Key(_comment: string): Promise<GeneratedKey> {
    return Promise.reject(new Error(unavailable));
  }

  parsePrivateKey(_pem: string, _passphrase: string | null): ParsedKey {
    return unsupported();
  }

  connect(_config: NativeConnectConfig): Promise<string> {
    return Promise.reject(new Error(unavailable));
  }

  exec(_sessionId: string, _command: string): Promise<NativeExecOutput> {
    return Promise.reject(new Error(unavailable));
  }

  disconnect(_sessionId: string): Promise<void> {
    return Promise.reject(new Error(unavailable));
  }

  openShell(_sessionId: string, _options: NativeShellOptions): Promise<string> {
    return Promise.reject(new Error(unavailable));
  }

  closeShell(_shellId: string): void {
    unsupported();
  }

  answerHostKey(_requestId: string, _accept: boolean): boolean {
    return unsupported();
  }

  answerPrompts(_requestId: string, _answers: string[] | null): boolean {
    return unsupported();
  }

  isRetryableCode(_code: string): boolean {
    return unsupported();
  }
}

export default registerWebModule(ReactNativeSshModule, 'ReactNativeSsh');
