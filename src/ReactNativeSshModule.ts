import { NativeModule, requireNativeModule } from 'expo';

import type { GeneratedKey } from './ReactNativeSsh.types';

declare class ReactNativeSshModule extends NativeModule<Record<string, never>> {
  coreVersion(): string;
  generateEd25519Key(comment: string): Promise<GeneratedKey>;
}

export default requireNativeModule<ReactNativeSshModule>('ReactNativeSsh');
