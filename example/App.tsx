import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import SshDemo from './SshDemo';
import SshTerminalDemo from './SshTerminalDemo';
import TerminalDemo from './TerminalDemo';

const DEMOS = [
  { key: 'ssh-terminal', label: 'SSH terminal', Screen: SshTerminalDemo },
  { key: 'terminal', label: 'PTY server terminal', Screen: TerminalDemo },
  { key: 'ssh', label: 'SSH commands', Screen: SshDemo },
] as const;

export default function App() {
  const [selected, setSelected] = useState<(typeof DEMOS)[number]['key']>('ssh-terminal');
  const current = DEMOS.find((demo) => demo.key === selected) ?? DEMOS[0];

  return (
    <View style={styles.container}>
      <View style={styles.tabs}>
        {DEMOS.map((demo) => (
          <Pressable
            key={demo.key}
            accessibilityRole="button"
            accessibilityState={{ selected: demo.key === selected }}
            onPress={() => setSelected(demo.key)}
            style={[styles.tab, demo.key === selected && styles.tabSelected]}>
            <Text style={styles.tabLabel}>{demo.label}</Text>
          </Pressable>
        ))}
      </View>
      <current.Screen />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#101418', paddingTop: 40 },
  tabs: { flexDirection: 'row', gap: 6, paddingHorizontal: 10, paddingBottom: 8 },
  tab: { flex: 1, paddingVertical: 8, alignItems: 'center', backgroundColor: '#1b2027', borderRadius: 6 },
  tabSelected: { backgroundColor: '#2f6f4f' },
  tabLabel: { color: '#d8dee9', fontSize: 12 },
});
