import { NativeModule, registerWebModule } from 'expo';

import type { GeneratedKey } from './ReactNativeSsh.types';

const unavailable = '@kilocode/react-native-ssh has no web implementation';

class ReactNativeSshModule extends NativeModule<Record<string, never>> {
  coreVersion(): string {
    throw new Error(unavailable);
  }

  generateEd25519Key(_comment: string): Promise<GeneratedKey> {
    return Promise.reject(new Error(unavailable));
  }
}

export default registerWebModule(ReactNativeSshModule, 'ReactNativeSshModule');
