import ExpCore
import SwiftUI

/// The ONE action glyph resolver: an action's stored curated `icon` when this
/// build ships an asset for it, else the generic action mark. A newer server
/// may store a name from a later icon set; drawing nothing for it left a
/// blank where every other row has a glyph, so an unknown name falls back
/// exactly like an unset one (the board twin is `BoardTypeDisplay`).
public enum ActionIconDisplay {
    public static func iconName(for icon: String?) -> String {
        BoardTypeDisplay.iconName(for: icon) ?? AppIcons.actionDefault
    }
}
