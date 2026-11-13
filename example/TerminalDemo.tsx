import { StatusBar } from 'expo-status-bar';
import { useEffect, useRef, useState } from 'react';
import { Linking, Pressable, StyleSheet, Text, View } from 'react-native';

import {
  createFeedSource,
  TerminalView,
  type FeedSource,
  type TerminalSize,
  type TerminalViewHandle,
} from '@kilocode/react-native-ssh';

const PTY_URL = process.env.EXPO_PUBLIC_PTY_URL ?? 'ws://localhost:7681';
const CTRL_C = 'Aw==';

export default function TerminalDemo() {
  const [sourceId, setSourceId] = useState<string | null>(null);
  const [fontSize, setFontSize] = useState(13);
  const [size, setSize] = useState<TerminalSize>({ cols: 0, rows: 0 });
  const [title, setTitle] = useState<string | null>(null);
  const [status, setStatus] = useState('connecting');

  const terminalRef = useRef<TerminalViewHandle>(null);
  const sourceRef = useRef<FeedSource | null>(null);
  const sizeRef = useRef<TerminalSize>({ cols: 80, rows: 24 });

  useEffect(() => {
    const source = createFeedSource();
    sourceRef.current = source;
    setSourceId(source.sourceId);

    const socket = new WebSocket(PTY_URL);
    socket.binaryType = 'arraybuffer';

    const sendControl = (message: object) => {
      if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
    };

    const stopInput = source.onInput((bytes) => {
      if (socket.readyState === WebSocket.OPEN) socket.send(bytes);
    });

    const stopResize = source.onResize((next) => {
      sizeRef.current = next;
      sendControl({ type: 'resize', ...next });
    });

    socket.onopen = () => {
      setStatus('connected');
      sendControl({ type: 'resize', ...sizeRef.current });
    };
    socket.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        source.feed(new Uint8Array(event.data));
      }
    };
    socket.onerror = () => setStatus('error');
    socket.onclose = () => {
      setStatus('closed');
      source.close(0, 'socket closed');
    };

    return () => {
      stopInput();
      stopResize();
      socket.close();
      source.close(0, 'unmounted');
    };
  }, []);

  return (
    <View style={styles.container}>
      <StatusBar style="light" />
      <View style={styles.header}>
        <Text style={styles.title} numberOfLines={1}>
          {title ?? 'terminal'}
        </Text>
        <Text style={styles.status}>
          {status} · {size.cols}x{size.rows} · {fontSize}pt
        </Text>
      </View>

      {sourceId ? (
        <TerminalView
          ref={terminalRef}
          style={styles.terminal}
          sourceId={sourceId}
          fontFamily="JetBrainsMono-Regular"
          fontSize={fontSize}
          onResize={setSize}
          onTitleChange={setTitle}
          onFontSizeChange={setFontSize}
          onLinkPress={(url) => {
            Linking.openURL(url).catch(() => undefined);
          }}
        />
      ) : null}

      <View style={styles.actions}>
        <Action label="Ctrl-C" onPress={() => terminalRef.current?.sendInput(CTRL_C)} />
        <Action
          label="Copy"
          onPress={() => {
            terminalRef.current?.copySelection();
          }}
        />
        <Action label="Paste" onPress={() => terminalRef.current?.paste('ls -la\r')} />
        <Action label="Bottom" onPress={() => terminalRef.current?.scrollToBottom()} />
      </View>
    </View>
  );
}

function Action({ label, onPress }: { label: string; onPress: () => void }) {
  return (
    <Pressable accessibilityRole="button" onPress={onPress} style={styles.action}>
      <Text style={styles.actionLabel}>{label}</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#101418', paddingTop: 56 },
  header: { paddingHorizontal: 12, paddingBottom: 8, gap: 2 },
  title: { color: '#d8dee9', fontSize: 14, fontWeight: '600' },
  status: { color: '#5a6472', fontSize: 11 },
  terminal: { flex: 1 },
  actions: { flexDirection: 'row', gap: 8, padding: 12, backgroundColor: '#1b2027' },
  action: { flex: 1, paddingVertical: 10, alignItems: 'center', backgroundColor: '#2a323d', borderRadius: 6 },
  actionLabel: { color: '#d8dee9', fontSize: 12 },
});
