import UIKit

extension NSAttributedString.Key {
    static let markdownHeadingLevel = NSAttributedString.Key("exp.markdownHeadingLevel")
    static let markdownListType = NSAttributedString.Key("exp.markdownListType")
    static let markdownListItemIndex = NSAttributedString.Key("exp.markdownListItemIndex")
    static let markdownListDepth = NSAttributedString.Key("exp.markdownListDepth")
    static let markdownCodeBlock = NSAttributedString.Key("exp.markdownCodeBlock")
    static let markdownCodeBlockLang = NSAttributedString.Key("exp.markdownCodeBlockLang")
    static let markdownBlockquote = NSAttributedString.Key("exp.markdownBlockquote")
    static let markdownInlineCode = NSAttributedString.Key("exp.markdownInlineCode")
    static let markdownImageURL = NSAttributedString.Key("exp.markdownImageURL")
    static let markdownImageAlt = NSAttributedString.Key("exp.markdownImageAlt")
    static let markdownStrikethrough = NSAttributedString.Key("exp.markdownStrikethrough")
}

enum MarkdownStyle {
    static let bodyFont = UIFont.preferredFont(forTextStyle: .body)
    static let textColor = UIColor.white.withAlphaComponent(0.9)
    static let linkColor = UIColor(red: 0.42, green: 0.64, blue: 1.0, alpha: 1.0)
    static let codeBackground = UIColor.white.withAlphaComponent(0.08)
    static let codeBlockBackground = UIColor.white.withAlphaComponent(0.06)
    static let blockquoteTextColor = UIColor.white.withAlphaComponent(0.6)
    static let placeholderColor = UIColor.white.withAlphaComponent(0.3)
    static let editorBackground = UIColor(white: 0.08, alpha: 1.0)

    static func headingFont(level: Int) -> UIFont {
        let sizes: [CGFloat] = [0, 24, 20, 18, 16, 15, 14]
        let size = level >= 1 && level <= 6 ? sizes[level] : bodyFont.pointSize
        return UIFont.systemFont(ofSize: size, weight: .semibold)
    }

    static var monospaceFont: UIFont {
        UIFont.monospacedSystemFont(ofSize: bodyFont.pointSize * 0.9, weight: .regular)
    }

    static var baseAttributes: [NSAttributedString.Key: Any] {
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.lineSpacing = 4
        return [
            .font: bodyFont,
            .foregroundColor: textColor,
            .paragraphStyle: paragraphStyle,
        ]
    }
}
