import ExpCore
import ExpUI
import ExponentialUI
import ExponentialUICore
import SwiftUI

// #EXP-1804 (web `components/ci/ci-surface.tsx`, Android `CiSurface`): a
// genUI tree the app rebuilds from live data (the CI run page, the CI runner
// section), painted by the core catalog like every other surface ×4. `tree`
// replaces the painted tree whenever it changes; `data` seeds the data model
// (bound fields); `onAction` gets every event (name, context, the
// component's payload).

/// The surface's data model, for a host writing back into it (a ChipInput's
/// bound `values`), and the latest action handler.
@MainActor
final class CiSurfaceHandle {
    fileprivate var model: SurfaceModel?
    fileprivate var onAction: (String, [String: CardJSON]?, [String: CardJSON]?) -> Void = { _, _, _ in }

    func setData(_ path: String, _ value: CardJSON) {
        model?.setData(path: path, value: JSONValue.parse(value.compactString))
    }
}

private extension JSONValue {
    var cardObject: [String: CardJSON]? { CardJSON(any: any)?.objectValue }
}

struct CiSurface: View {
    let surfaceId: String
    let tree: CardJSON
    var data: [String: CardJSON] = [:]
    var handle: CiSurfaceHandle?
    let onAction: (String, [String: CardJSON]?, [String: CardJSON]?) -> Void

    @State private var own = CiSurfaceHandle()
    @State private var model: SurfaceModel?

    private var box: CiSurfaceHandle { handle ?? own }

    var body: some View {
        // The latest handler, read when an event fires.
        let _ = { box.onAction = onAction }()
        Group {
            if let model {
                ExponentialSurface(model: model)
            } else {
                Color.clear.frame(height: 1)
            }
        }
        .task { build() }
        .onChange(of: tree) { _, next in
            _ = try? model?.setNested(json: next.compactString)
        }
    }

    private func build() {
        guard model == nil else { return }
        let target = box
        let host = ClosureHost(
            icons: { name, size in
                guard let resolved = AppIcons.conceptOrName(name) else { return nil }
                return AnyView(AppIcon(resolved, size: size))
            },
            actions: { event in
                target.onAction(event.name, event.context.cardObject, event.payload?.cardObject)
            }
        )
        guard let next = try? SurfaceModel(
            id: surfaceId,
            options: SurfaceOptions(theme: ThemeHandle.builtin("exponential"), mode: .dark),
            host: host
        ) else { return }
        _ = try? next.setNested(json: tree.compactString)
        for (key, value) in data {
            next.setData(path: "/" + key, value: JSONValue.parse(value.compactString))
        }
        target.model = next
        model = next
    }
}
