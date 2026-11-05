import CoreGraphics
import Foundation

let pid = Int(CommandLine.arguments[1])!
let windows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as! [[String: Any]]
let visible = windows.filter { window in
    guard window[kCGWindowOwnerPID as String] as? Int == pid,
          window[kCGWindowLayer as String] as? Int == 0,
          let bounds = window[kCGWindowBounds as String] as? [String: Any],
          let width = bounds["Width"] as? Double else { return false }
    return width > 100
}
let action = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : "count"
if action == "bounds" {
    let bounds = visible.first![kCGWindowBounds as String]!
    let data = try! JSONSerialization.data(withJSONObject: bounds)
    print(String(data: data, encoding: .utf8)!)
} else if action == "move" {
    let point = CGPoint(x: Double(CommandLine.arguments[3])!, y: Double(CommandLine.arguments[4])!)
    CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
} else {
    print(visible.count)
}
