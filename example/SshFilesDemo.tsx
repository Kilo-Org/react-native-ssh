import {
  SshError,
  addSshListener,
  answerHostKey,
  closeSftp,
  connect,
  disconnect,
  download,
  exec,
  isRetryableError,
  listDirectory,
  openSftp,
  startLocalForward,
} from '@kilocode/react-native-ssh';
import type { Forward } from '@kilocode/react-native-ssh';
import { useEffect, useState } from 'react';
import { Alert, Button, ScrollView, StyleSheet, Text, View } from 'react-native';

/**
 * Drives SFTP and a local forward against the containers published by
 * `rust/ssh-core/tests/docker` (`adb reverse tcp:2222 tcp:2222` in the example
 * app). The forward's destination is resolved from the server, so it names the
 * `http` container.
 *
 * The download target is the example app's own sandbox; an app reads the
 * directory from `expo-file-system` and appends the file name.
 */
const SERVER = {
  host: '127.0.0.1',
  port: 2222,
  username: 'user',
  password: 'ssh-core-password',
};
const REMOTE_DIR = '/home/user';
const REMOTE_SAMPLE = `${REMOTE_DIR}/sftp-demo.txt`;
const SANDBOX_FILES = 'file:///data/user/0/ai.kilo.reactnativessh.example/files';
const HTTP_FIXTURE = { host: 'ssh-core-http', port: 8080 };

export default function SshFilesDemo() {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [sftpId, setSftpId] = useState<string | null>(null);
  const [lines, setLines] = useState<string[]>([]);
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
      addSshListener('onForwardError', (notice) => {
        append(`forward error: ${notice.message}`);
      }).remove,
    []
  );

  function append(line: string) {
    setLines((current) => [...current, line]);
  }

  async function openSession() {
    setError(null);
    setLines([]);

    try {
      const id = await connect({
        host: SERVER.host,
        port: SERVER.port,
        username: SERVER.username,
        auth: { type: 'password', password: SERVER.password },
      });
      setSessionId(id);
      setSftpId(await openSftp(id));
      append('session and SFTP channel open');
    } catch (cause) {
      setError(describe(cause));
    }
  }

  async function listRemote() {
    if (!sftpId) {
      return;
    }

    try {
      const entries = await listDirectory(sftpId, REMOTE_DIR);
      entries.forEach((entry) =>
        append(`${entry.kind} ${entry.size} ${entry.name}`)
      );
    } catch (cause) {
      setError(describe(cause));
    }
  }

  async function downloadSample() {
    if (!sftpId || !sessionId) {
      return;
    }

    try {
      await exec(sessionId, `printf 'hello from sftp\\n' > ${REMOTE_SAMPLE}`);
      const localUrl = `${SANDBOX_FILES}/sftp-demo.txt`;
      const written = await download(sftpId, REMOTE_SAMPLE, localUrl, {
        onProgress: (progress) => {
          append(`downloaded ${progress.transferred}/${progress.total ?? '?'} bytes`);
        },
      });
      append(`download complete: ${written} bytes at ${localUrl}`);
    } catch (cause) {
      setError(describe(cause));
    }
  }

  async function fetchThroughForward() {
    if (!sessionId) {
      return;
    }

    let forward: Forward | null = null;

    try {
      forward = await startLocalForward(sessionId, {
        destHost: HTTP_FIXTURE.host,
        destPort: HTTP_FIXTURE.port,
      });
      append(`forward listening on 127.0.0.1:${forward.port}`);

      const response = await fetch(`http://127.0.0.1:${forward.port}/`);
      append(`forward response: ${await response.text()}`);
    } catch (cause) {
      setError(describe(cause));
    } finally {
      await forward?.stop();
    }
  }

  async function closeSession() {
    if (sftpId) {
      await closeSftp(sftpId);
      setSftpId(null);
    }
    if (sessionId) {
      await disconnect(sessionId);
      setSessionId(null);
    }
  }

  return (
    <View style={styles.container}>
      <ScrollView contentContainerStyle={styles.content}>
        <Text style={styles.heading}>SFTP and forwarding</Text>
        <Text testID="sftp-session">{sessionId ?? 'not connected'}</Text>

        <Button title="Connect" onPress={openSession} disabled={!!sessionId} />
        <Button
          accessibilityLabel="List remote directory"
          title="List remote directory"
          onPress={listRemote}
          disabled={!sftpId}
        />
        <Button
          accessibilityLabel="Download sample file"
          title="Download sample file"
          onPress={downloadSample}
          disabled={!sftpId}
        />
        <Button
          accessibilityLabel="Fetch through local forward"
          title="Fetch through local forward"
          onPress={fetchThroughForward}
          disabled={!sessionId}
        />
        <Button title="Disconnect" onPress={closeSession} disabled={!sessionId} />

        {lines.map((line, index) => (
          <Text key={`${index}-${line}`} style={styles.output} testID="sftp-output">
            {line}
          </Text>
        ))}

        {error ? (
          <Text style={styles.error} testID="sftp-error">
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
