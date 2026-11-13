import Foundation

/// Process-wide lookup from the `sourceId` prop to the `TerminalByteSource` that serves it.
///
/// Views subscribe while they are attached so a source registered after the view mounts still binds.
final class TerminalSourceRegistry {
  static let shared = TerminalSourceRegistry()

  private var sources: [String: TerminalByteSource] = [:]
  private var listeners: [UUID: () -> Void] = [:]

  func register(_ sourceId: String, _ source: TerminalByteSource) {
    sources[sourceId] = source
    notifyListeners()
  }

  func unregister(_ sourceId: String) {
    sources.removeValue(forKey: sourceId)
    notifyListeners()
  }

  func find(_ sourceId: String) -> TerminalByteSource? {
    return sources[sourceId]
  }

  @discardableResult
  func addListener(_ listener: @escaping () -> Void) -> UUID {
    let token = UUID()
    listeners[token] = listener
    return token
  }

  func removeListener(_ token: UUID) {
    listeners.removeValue(forKey: token)
  }

  private func notifyListeners() {
    DispatchQueue.main.async { [weak self] in
      self?.listeners.values.forEach { $0() }
    }
  }
}
