import ExpoModulesCore
import SwiftTerm
import UIKit

final class TerminalThemeRecord: Record {
  @Field var foreground: Int = 0
  @Field var background: Int = 0
  @Field var cursor: Int = 0
  @Field var selection: Int = 0
  @Field var ansi: [Int] = []
}

struct TerminalTheme {
  let foreground: UIColor
  let background: UIColor
  let cursor: UIColor
  let selection: UIColor
  let ansi: [SwiftTerm.Color]

  static let `default` = TerminalTheme(
    foreground: 0xFFD8DEE9,
    background: 0xFF101418,
    cursor: 0xFFD8DEE9,
    selection: 0x553B82C4,
    ansi: [
      0xFF1B2027, 0xFFD06A6A, 0xFF8FBF7F, 0xFFD6B36A,
      0xFF6C9FD8, 0xFFB48EAD, 0xFF6FB6BF, 0xFFC3C9D4,
      0xFF5A6472, 0xFFE08A8A, 0xFFA8D49A, 0xFFE7CB88,
      0xFF8AB8E8, 0xFFC9A6CD, 0xFF8FD0D8, 0xFFF0F3F7,
    ]
  )

  init(foreground: Int, background: Int, cursor: Int, selection: Int, ansi: [Int]) {
    self.foreground = UIColor(argb: foreground)
    self.background = UIColor(argb: background)
    self.cursor = UIColor(argb: cursor)
    self.selection = UIColor(argb: selection)
    self.ansi = ansi.prefix(16).map { SwiftTerm.Color(argb: $0) }
  }

  init(record: TerminalThemeRecord) {
    self.init(
      foreground: record.foreground,
      background: record.background,
      cursor: record.cursor,
      selection: record.selection,
      ansi: record.ansi
    )
  }

  func apply(to view: TerminalView) {
    view.installColors(ansi)
    view.nativeForegroundColor = foreground
    view.nativeBackgroundColor = background
    view.caretColor = cursor
    view.selectedTextBackgroundColor = selection
    view.backgroundColor = background
  }
}

private extension UIColor {
  convenience init(argb: Int) {
    let value = UInt32(truncatingIfNeeded: argb)
    self.init(
      red: CGFloat((value >> 16) & 0xFF) / 255,
      green: CGFloat((value >> 8) & 0xFF) / 255,
      blue: CGFloat(value & 0xFF) / 255,
      alpha: CGFloat((value >> 24) & 0xFF) / 255
    )
  }
}

private extension SwiftTerm.Color {
  convenience init(argb: Int) {
    let value = UInt32(truncatingIfNeeded: argb)
    self.init(
      red: UInt16((value >> 16) & 0xFF) * 257,
      green: UInt16((value >> 8) & 0xFF) * 257,
      blue: UInt16(value & 0xFF) * 257
    )
  }
}
