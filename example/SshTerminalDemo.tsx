import {
  addSshListener,
  answerHostKey,
  closeShell,
  connect,
  disconnect,
  openShell,
  TerminalView,
  type ConnectOptions,
  type TerminalSize,
} from '@kilocode/react-native-ssh';
import { StatusBar } from 'expo-status-bar';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, Pressable, StyleSheet, Text, View } from 'react-native';

const SERVER: ConnectOptions = {
  host: '127.0.0.1',
  port: 2222,
  username: 'user',
  auth: { type: 'password', password: 'ssh-core-password' },
};

export default function SshTerminalDemo() {
  const [sourceId, setSourceId] = useState<string | null>(null);
  const [status, setStatus] = useState('idle');
  const [size, setSize] = useState<TerminalSize>({ cols: 0, rows: 0 });
  const [title, setTitle] = useState<string | null>(null);
  const [fontSize, setFontSize] = useState(13);

  const sessionRef = useRef<string | null>(null);
  const shellRef = useRef<string | null>(null);

  useEffect(
    () =>
      addSshListener('onHostKey', (request) => {
        console.log('[ssh-terminal] host key', request.algorithm, request.fingerprintSha256);
        Alert.alert(
          'Unknown host key',
          `${request.algorithm}\n${request.fingerprintSha256}`,
          [
            { text: 'Reject', style: 'cancel', onPress: () => answerHostKey(request.requestId, false) },
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
        console.log('[ssh-terminal] disconnect', notice.reason);
        setStatus(`disconnected: ${notice.reason}`);
        setSourceId(null);
      }).remove,
    []
  );

  const open = useCallback(async () => {
    try {
      setStatus('connecting');
      const sessionId = await connect(SERVER);
      sessionRef.current = sessionId;

      const shellId = await openShell(sessionId, { term: 'xterm-256color', cols: 80, rows: 24 });
      shellRef.current = shellId;
      setSourceId(shellId);
      setStatus('shell open');
      console.log('[ssh-terminal] shell source', shellId);
    } catch (cause) {
      setStatus(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  const close = useCallback(() => {
    const shellId = shellRef.current;
    const sessionId = sessionRef.current;
    shellRef.current = null;
    sessionRef.current = null;
    setSourceId(null);
    setStatus('closed');
    if (shellId) closeShell(shellId);
    if (sessionId) disconnect(sessionId).catch(() => undefined);
  }, []);

  useEffect(() => () => {
    const shellId = shellRef.current;
    const sessionId = sessionRef.current;
    if (shellId) closeShell(shellId);
    if (sessionId) disconnect(sessionId).catch(() => undefined);
  }, []);

  return (
    <View style={styles.container}>
      <StatusBar style="light" />
      <View style={styles.header}>
        <Text style={styles.title} numberOfLines={1}>
          {title ?? 'ssh terminal'}
        </Text>
        <Text style={styles.status}>
          {status} · {size.cols}x{size.rows} · {fontSize}pt
        </Text>
      </View>

      {sourceId ? (
        <TerminalView
          style={styles.terminal}
          sourceId={sourceId}
          fontFamily="JetBrainsMono-Regular"
          fontSize={fontSize}
          onResize={(next) => {
            console.log('[ssh-terminal] resize', next.cols, next.rows);
            setSize(next);
          }}
          onTitleChange={(next) => {
            console.log('[ssh-terminal] title', next);
            setTitle(next);
          }}
          onFontSizeChange={(next) => {
            console.log('[ssh-terminal] font size', next);
            setFontSize(next);
          }}
          onSelectionChange={(selecting) => console.log('[ssh-terminal] selection', selecting)}
          onLinkPress={(url) => console.log('[ssh-terminal] link', url)}
          onBell={() => console.log('[ssh-terminal] bell')}
        />
      ) : (
        <View style={styles.terminal} />
      )}

      <View style={styles.actions}>
        <Pressable accessibilityRole="button" onPress={open} style={styles.action}>
          <Text style={styles.actionLabel}>Connect</Text>
        </Pressable>
        <Pressable accessibilityRole="button" onPress={close} style={styles.action}>
          <Text style={styles.actionLabel}>Close</Text>
        </Pressable>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#101418' },
  header: { paddingHorizontal: 12, paddingBottom: 6, gap: 2 },
  title: { color: '#d8dee9', fontSize: 14, fontWeight: '600' },
  status: { color: '#5a6472', fontSize: 11 },
  terminal: { flex: 1 },
  actions: { flexDirection: 'row', gap: 8, padding: 10, backgroundColor: '#1b2027' },
  action: { flex: 1, paddingVertical: 10, alignItems: 'center', backgroundColor: '#2a323d', borderRadius: 6 },
  actionLabel: { color: '#d8dee9', fontSize: 12 },
});
