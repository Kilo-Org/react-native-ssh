import {
  SshError,
  addSshListener,
  answerHostKey,
  connect,
  disconnect,
  exec,
  isRetryableError,
} from '@kilocode/react-native-ssh';
import { useEffect, useState } from 'react';
import { Alert, Button, ScrollView, StyleSheet, Text, View } from 'react-native';

/**
 * Connects to the test server published by `rust/ssh-core/tests/docker`
 * (`adb reverse tcp:2222 tcp:2222` in the example app), asks for the host key
 * in an alert, then runs `uname -a` over the session.
 */
const SERVER = {
  host: '127.0.0.1',
  port: 2222,
  username: 'user',
  password: 'ssh-core-password',
};

export default function SshDemo() {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [output, setOutput] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(
    () =>
      addSshListener('onHostKey', (request) => {
        Alert.alert(
          'Unknown host key',
          `${request.algorithm}\n${request.fingerprintSha256}`,
          [
            {
              text: 'Reject',
              style: 'cancel',
              onPress: () => answerHostKey(request.requestId, false),
            },
            { text: 'Accept', onPress: () => answerHostKey(request.requestId, true) },
          ],
          { cancelable: false }
        );
      }).remove,
    []
  );

  useEffect(
    () =>
      addSshListener('onDisconnect', (notice) => {
        setOutput((lines) => [...lines, `disconnected: ${notice.reason}`]);
        setSessionId((current) => (current === notice.sessionId ? null : current));
      }).remove,
    []
  );

  async function runCommand() {
    setError(null);
    setOutput([]);

    try {
      const id = await connect({
        host: SERVER.host,
        port: SERVER.port,
        username: SERVER.username,
        auth: { type: 'password', password: SERVER.password },
      });
      setSessionId(id);

      const result = await exec(id, 'uname -a');
      setOutput([result.stdout.trim(), `exit ${result.exitStatus ?? 'unknown'}`]);
    } catch (cause) {
      setError(describe(cause));
    }
  }

  async function closeSession() {
    if (!sessionId) {
      return;
    }

    await disconnect(sessionId);
    setSessionId(null);
  }

  return (
    <View style={styles.container}>
      <ScrollView contentContainerStyle={styles.content}>
        <Text style={styles.heading}>SSH session</Text>
        <Text testID="session-id">{sessionId ?? 'not connected'}</Text>

        <Button title="Connect and run uname -a" onPress={runCommand} />
        <Button title="Disconnect" onPress={closeSession} disabled={!sessionId} />

        {output.map((line) => (
          <Text key={line} style={styles.output} testID="ssh-output">
            {line}
          </Text>
        ))}

        {error ? (
          <Text style={styles.error} testID="ssh-error">
            {error}
          </Text>
        ) : null}
      </ScrollView>
    </View>
  );
}

function describe(cause: unknown): string {
  if (cause instanceof SshError) {
    return `${cause.code} (retryable: ${isRetryableError(cause)}): ${cause.message}`;
  }

  return cause instanceof Error ? cause.message : String(cause);
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#f2f2f2', paddingTop: 72 },
  content: { padding: 20, gap: 12 },
  heading: { fontSize: 16, fontWeight: '600' },
  output: { fontFamily: 'Courier', fontSize: 12 },
  error: { color: '#b00020' },
});
