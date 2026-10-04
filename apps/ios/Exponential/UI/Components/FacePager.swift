import SwiftUI

/// The pager under a tab strip: a horizontal PAGING scroll view, one
/// full-size page per tag, mounted as it comes into reach. The scroll view
/// arbitrates a nested vertical feed and a sideways code scroller itself.
///
/// EXP-1160: deliberately NOT `TabView` in its `.page` style, which this was
/// until then. On the page a screen opened on, the first `.sheet` presented
/// from inside the page was presented TWICE (UIKit: "already presenting") and
/// both were torn down again — the first tap on a property chip did nothing —
/// and its pages lost the safe area (title jammed under the header band, the
/// bottom bar over the home indicator). A scroll view's pages are plain
/// children: sheets, alerts and `safeAreaInset` bars behave as on any screen,
/// so a page presents its own sheets like every other view does.
struct FacePager<Tag: Hashable, Page: View>: View {
    let pages: [Tag]
    @Binding var selection: Tag
    @ViewBuilder let page: (Tag) -> Page

    var body: some View {
        // The scroll view spans the WHOLE screen and every page gets the safe
        // area back as its own (the header band above, the home indicator or
        // the keyboard below, the side insets in landscape). Left to the
        // scroll view, the cross-axis safe area is OS-dependent (iOS 18 sizes
        // a page to the full container and pushes its bottom bar off screen)
        // and the neighbour page shows through the side insets.
        GeometryReader { geometry in
            let insets = geometry.safeAreaInsets
            ScrollViewReader { scroller in
                ScrollView(.horizontal) {
                    LazyHStack(spacing: 0) {
                        ForEach(pages, id: \.self) { tag in
                            page(tag)
                                .frame(maxWidth: .infinity, maxHeight: .infinity)
                                .safeAreaPadding(insets)
                                .frame(
                                    width: geometry.size.width + insets.leading + insets.trailing,
                                    height: geometry.size.height + insets.top + insets.bottom
                                )
                        }
                    }
                    .scrollTargetLayout()
                }
                .scrollTargetBehavior(.paging)
                .scrollPosition(id: Binding(
                    get: { selection },
                    set: { if let next = $0, next != selection { selection = next } }
                ))
                .scrollIndicators(.hidden)
                .ignoresSafeArea()
                // A page that arrives BEFORE the shown one (a run's issue
                // syncing in) shifts the row under the viewport: land on the
                // selection again, unanimated.
                .onChange(of: pages) {
                    scroller.scrollTo(selection, anchor: .leading)
                }
                // And a new width (rotation, an iPad split) moves every page's
                // origin while the offset stays put — once the pages took
                // their new size, not before.
                .onChange(of: geometry.size.width) {
                    DispatchQueue.main.async {
                        scroller.scrollTo(selection, anchor: .leading)
                    }
                }
            }
        }
    }
}
