// Draws each character of a TrueType font with macOS Core Text, without
// antialiasing, at `scale` device pixels per font pixel, and prints the glyph as
// rows of `#` and `.` sampled at the centre of each font pixel, with its advance.
// tests/font.rs compares the output with the spritebox glyphs.
//
//     swift scripts/check-ttf.swift font.ttf HEIGHT BASELINE SCALE CHARS
import CoreGraphics
import CoreText
import Foundation

let args = CommandLine.arguments
guard args.count == 6,
      let height = Int(args[2]), let baseline = Int(args[3]), let scale = Int(args[4])
else {
    FileHandle.standardError.write("usage: check-ttf.swift font.ttf HEIGHT BASELINE SCALE CHARS\n".data(using: .utf8)!)
    exit(2)
}
let url = URL(fileURLWithPath: args[1]) as CFURL
guard let descriptors = CTFontManagerCreateFontDescriptorsFromURL(url) as? [CTFontDescriptor],
      let descriptor = descriptors.first
else {
    print("error: Core Text cannot read the font")
    exit(1)
}
// The em is the glyph height, so this size gives `scale` pixels per font pixel.
let font = CTFontCreateWithFontDescriptor(descriptor, CGFloat(height * scale), nil)
print("family \(CTFontCopyFamilyName(font) as String)")
print("glyphs \(CTFontGetGlyphCount(font))")
for scalar in args[5].unicodeScalars {
    var chars = Array(String(scalar).utf16)
    var glyphs = [CGGlyph](repeating: 0, count: chars.count)
    guard CTFontGetGlyphsForCharacters(font, &chars, &glyphs, chars.count) else {
        print("missing \(scalar.value)")
        continue
    }
    var advance = CGSize.zero
    CTFontGetAdvancesForGlyphs(font, .horizontal, &glyphs, &advance, 1)
    let width = Int((advance.width / CGFloat(scale)).rounded()) + 2
    let w = width * scale, h = height * scale
    let context = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w,
                            space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
    context.setShouldAntialias(false)
    context.setAllowsFontSmoothing(false)
    context.setFillColor(gray: 0, alpha: 1)
    context.fill(CGRect(x: 0, y: 0, width: w, height: h))
    context.setFillColor(gray: 1, alpha: 1)
    var position = CGPoint(x: 0, y: CGFloat((height - baseline) * scale))
    CTFontDrawGlyphs(font, &glyphs, &position, 1, context)
    let data = context.data!.assumingMemoryBound(to: UInt8.self)
    print("glyph \(scalar.value) advance \(Double(advance.width) / Double(scale))")
    for row in 0..<height {
        var line = ""
        for x in 0..<width {
            // Bitmap rows run from the top; sample the centre of the font pixel.
            let px = x * scale + scale / 2, py = row * scale + scale / 2
            line += data[py * w + px] > 127 ? "#" : "."
        }
        print(line)
    }
}
