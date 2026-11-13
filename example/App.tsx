import { StatusBar } from 'expo-status-bar';
import { StyleSheet, View } from 'react-native';

import TerminalDemo from './TerminalDemo';

export default function App() {
  return (
    <View style={styles.container}>
      <StatusBar style="light" />
      <TerminalDemo />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#101418' },
});
