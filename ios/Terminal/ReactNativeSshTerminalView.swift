import ExpoModulesCore
import SwiftTerm
import UIKit

private let transcriptRows = 10_000
private let defaultFontFamily = "JetBrainsMono-Regular"
private let defaultFontSize: CGFloat = 13
private let minFontSize: CGFloat = 7
private let maxFontSize: CGFloat = 36
private let zoomStep: CGFloat = 1.15

final class SshTerminalView: TerminalView {
  var onSelectionChanged: ((Bool) -> Void)?

  override init(frame: CGRect) {
    super.init(frame: frame)
  }

  required init?(coder: NSCoder) {
    super.init(coder: coder)
  }

  override func selectionChanged(source: Terminal) {
    super.selectionChanged(source: source)
    onSelectionChanged?(selectionActive)
  }
}

public final class ReactNativeSshTerminalView: ExpoView, TerminalViewDelegate, TerminalSink {
  public let onResize = EventDispatcher()
  public let onLinkPress = EventDispatcher()
  public let onBell = EventDispatcher()
  public let onTitleChange = EventDispatcher()
  public let onSelectionChange = EventDispatcher()
  public let onFontSizeChange = EventDispatcher()

  private let terminalView = SshTerminalView(frame: .zero)
  private var sourceId: String?
  private var source: TerminalByteSource?
  private var listenerToken: UUID?
  private var theme = TerminalTheme.default
  private var fontFamily = defaultFontFamily
  private var fontSize = defaultFontSize
  private var cursorStyle: CursorStyle = .steadyBlock
  private var scaleAccumulator: CGFloat = 1
  private var appliedCols = 0
  private var appliedRows = 0
  private var configuredScrollback = false

  public required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)

    terminalView.translatesAutoresizingMaskIntoConstraints = false
    terminalView.terminalDelegate = self
    terminalView.onSelectionChanged = { [weak self] selecting in
      self?.onSelectionChange(["selecting": selecting])
    }
    addSubview(terminalView)
    NSLayoutConstraint.activate([
      terminalView.leadingAnchor.constraint(equalTo: leadingAnchor),
      terminalView.trailingAnchor.constraint(equalTo: trailingAnchor),
      terminalView.topAnchor.constraint(equalTo: topAnchor),
      terminalView.bottomAnchor.constraint(equalTo: bottomAnchor),
    ])

    let pinch = UIPinchGestureRecognizer(target: self, action: #selector(handlePinch))
    addGestureRecognizer(pinch)

    applyFont()
    theme.apply(to: terminalView)
  }

  public override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil {
      if let listenerToken {
        TerminalSourceRegistry.shared.removeListener(listenerToken)
        self.listenerToken = nil
      }
      source?.setSink(nil)
      source = nil
      return
    }

    if listenerToken == nil {
      listenerToken = TerminalSourceRegistry.shared.addListener { [weak self] in
        self?.rebindSource()
      }
    }
    rebindSource()
  }

  func bindSource(_ sourceId: String?) {
    self.sourceId = sourceId
    rebindSource()
  }

  func applyFontFamily(_ fontFamily: String?) {
    self.fontFamily = fontFamily.flatMap { $0.isEmpty ? nil : $0 } ?? defaultFontFamily
    applyFont()
  }

  func applyFontSize(_ fontSize: Double) {
    self.fontSize = CGFloat(fontSize).clamped(to: minFontSize...maxFontSize)
    applyFont()
  }

  func applyTheme(_ record: TerminalThemeRecord) {
    theme = TerminalTheme(record: record)
    theme.apply(to: terminalView)
  }

  func applyCursorStyle(_ cursorStyle: String) {
    switch cursorStyle {
    case "bar":
      self.cursorStyle = .steadyBar
    case "underline":
      self.cursorStyle = .steadyUnderline
    default:
      self.cursorStyle = .steadyBlock
    }
    if appliedCols > 0 {
      terminalView.getTerminal().options.cursorStyle = self.cursorStyle
    }
  }

  func sendInput(base64: String) {
    guard let bytes = Data(base64Encoded: base64) else { return }
    terminalView.send(data: bytes[...])
  }

  func paste(text: String) {
    terminalView.send(txt: text)
  }

  func copySelection() {
    terminalView.copy(nil)
  }

  func scrollToBottom() {
    terminalView.scroll(toPosition: 1.0)
  }

  func onData(_ bytes: Data) {
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      self.terminalView.feed(byteArray: bytes[...])
    }
  }

  func onClosed(exitStatus: Int?, error: String?) {
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      var description = "\r\n[Connection closed"
      if let error {
        description += ": \(error)"
      } else if let exitStatus, exitStatus != 0 {
        description += " (code \(exitStatus))"
      }
      description += "]\r\n"
      self.terminalView.feed(text: description)
    }
  }

  public func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
    appliedCols = newCols
    appliedRows = newRows
    self.source?.resize(cols: newCols, rows: newRows)
    onResize(["cols": newCols, "rows": newRows])

    if !configuredScrollback {
      configuredScrollback = true
      let terminal = terminalView.getTerminal()
      terminal.options.scrollback = transcriptRows
      terminal.options.cursorStyle = cursorStyle
    }
  }

  public func setTerminalTitle(source: TerminalView, title: String) {
    onTitleChange(["title": title])
  }

  public func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {
  }

  public func send(source: TerminalView, data: ArraySlice<UInt8>) {
    self.source?.write(Data(data))
  }

  public func scrolled(source: TerminalView, position: Double) {
  }

  public func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
    onLinkPress(["url": link])
  }

  public func bell(source: TerminalView) {
    onBell([:])
  }

  public func clipboardCopy(source: TerminalView, content: Data) {
    UIPasteboard.general.string = String(decoding: content, as: UTF8.self)
  }

  public func iTermContent(source: TerminalView, content: ArraySlice<UInt8>) {
  }

  public func rangeChanged(source: TerminalView, startY: Int, endY: Int) {
  }

  @objc private func handlePinch(_ recognizer: UIPinchGestureRecognizer) {
    guard recognizer.state == .changed else { return }
    scaleAccumulator *= recognizer.scale
    recognizer.scale = 1

    if scaleAccumulator >= zoomStep {
      changeFontSize(by: 1)
    } else if scaleAccumulator <= 1 / zoomStep {
      changeFontSize(by: -1)
    }
  }

  private func changeFontSize(by delta: CGFloat) {
    let next = (fontSize + delta).clamped(to: minFontSize...maxFontSize)
    guard next != fontSize else { return }

    fontSize = next
    applyFont()
    onFontSizeChange(["fontSize": Int(next)])
    scaleAccumulator = 1
  }

  private func applyFont() {
    let font = UIFont(name: fontFamily, size: fontSize)
      ?? UIFont.monospacedSystemFont(ofSize: fontSize, weight: .regular)
    terminalView.font = font
  }

  private func rebindSource() {
    let next = sourceId.flatMap { TerminalSourceRegistry.shared.find($0) }
    if next === source { return }

    source?.setSink(nil)
    source = next
    next?.setSink(self)

    if next != nil && appliedCols > 0 {
      next?.resize(cols: appliedCols, rows: appliedRows)
    }
  }
}

private extension CGFloat {
  func clamped(to range: ClosedRange<CGFloat>) -> CGFloat {
    return Swift.min(Swift.max(self, range.lowerBound), range.upperBound)
  }
}
