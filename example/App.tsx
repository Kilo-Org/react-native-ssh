import { useEffect, useState } from 'react';
import { SafeAreaView, ScrollView, StyleSheet, Text } from 'react-native';

import ReactNativeSsh, { type GeneratedKey } from '@kilocode/react-native-ssh';

export default function App() {
  const [version, setVersion] = useState<string | null>(null);
  const [key, setKey] = useState<GeneratedKey | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    try {
      setVersion(ReactNativeSsh.coreVersion());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      return;
    }

    ReactNativeSsh.generateEd25519Key('react-native-ssh-example')
      .then((generated) => {
        if (!cancelled) {
          setKey(generated);
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <SafeAreaView style={styles.container}>
      <ScrollView contentContainerStyle={styles.content}>
        <Text style={styles.heading}>Rust core version</Text>
        <Text testID="core-version">{version ?? 'loading'}</Text>

        {key ? (
          <>
            <Text style={styles.heading}>Generated Ed25519 key</Text>
            <Text testID="public-key">{key.publicKeyOpenssh}</Text>
            <Text testID="fingerprint">{key.fingerprintSha256}</Text>
          </>
        ) : null}

        {error ? <Text testID="error">{error}</Text> : null}
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#f2f2f2' },
  content: { padding: 20, gap: 12 },
  heading: { fontSize: 16, fontWeight: '600' },
});
