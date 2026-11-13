import Foundation

typealias EventEmitter = (String, [String: Any?]) -> Void

final class TransferProgressSink: TransferProgress, @unchecked Sendable {
  private let transferId: String
  private let emit: EventEmitter

  init(transferId: String, emit: @escaping EventEmitter) {
    self.transferId = transferId
    self.emit = emit
  }

  func onProgress(transferred: UInt64, total: UInt64?) {
    emit("onTransferProgress", [
      "transferId": transferId,
      "transferred": Double(transferred),
      "total": total.map(Double.init),
    ])
  }
}

final class ForwardErrorSink: ForwardSink, @unchecked Sendable {
  private let sessionId: String
  private let emit: EventEmitter

  init(sessionId: String, emit: @escaping EventEmitter) {
    self.sessionId = sessionId
    self.emit = emit
  }

  func onError(message: String) {
    emit("onForwardError", ["sessionId": sessionId, "message": message])
  }
}
