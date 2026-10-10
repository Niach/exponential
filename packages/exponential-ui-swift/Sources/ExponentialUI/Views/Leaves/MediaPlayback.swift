import AVFoundation
import Observation

/// One Video / AudioPlayer's player (VAPP-103): its `src` plays ONLY through
/// the surface's policed media request (`SurfaceModel.mediaRequest`: the
/// host's resolveUrl, then the media policy); a denied src never reaches a
/// player and the leaf stays the static poster / controls. Nothing loads
/// before playback starts (a press, or `autoplay`); an http(s) request
/// streams with its headers, a `data:` one plays from a temporary file
/// (`MediaLoader.playable`). ONE open runs at a time: a second play of the
/// same request while it opens waits for it (`loading`), another request
/// replaces it.
@MainActor
@Observable
final class MediaPlayback {
    /// The player once a policed source opened (nil = not started, denied
    /// or failed).
    private(set) var player: AVPlayer?
    private(set) var playing = false
    /// True while a source opens (the play button waits).
    private(set) var loading = false
    /// Seconds played and the item's length (nil until it is known).
    private(set) var time: Double = 0
    private(set) var duration: Double?
    /// The request playback opened (tests).
    @ObservationIgnored private(set) var openedKey: String?
    /// How many players this playback built (tests).
    @ObservationIgnored private(set) var playersBuilt = 0
    @ObservationIgnored private var opening: (generation: Int, task: Task<Void, Never>)?
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var source: MediaLoader.Playable?
    @ObservationIgnored private var timeObserver: Any?
    @ObservationIgnored private var endObserver: NSObjectProtocol?
    let kind: MediaKind

    init(kind: MediaKind = .video) {
        self.kind = kind
    }

    /// Start playing `request` (muted for `autoplay`); a later request
    /// replaces the player. nil (a denied src) stops and loads nothing.
    /// `options` = the media policy a redirect passes.
    func play(_ request: URLRequest?, muted: Bool = false, options: MediaOptions? = nil) async {
        guard let request else { return stop() }
        let key = MediaLoader.key(request)
        if key == openedKey {
            if let player {
                player.play()
                playing = true
                return
            }
            if let opening {
                await opening.task.value
                return
            }
        }
        stop()
        openedKey = key
        loading = true
        generation += 1
        let gen = generation
        let task = Task { await self.open(request, generation: gen, muted: muted, options: options) }
        opening = (gen, task)
        await task.value
        if opening?.generation == gen {
            opening = nil
            loading = false
        }
    }

    private func open(_ request: URLRequest, generation gen: Int, muted: Bool, options: MediaOptions?) async {
        let got = await MediaLoader.shared.playable(request, kind: kind, options: options)
        guard gen == generation, !Task.isCancelled, let got else {
            MediaLoader.shared.release(got)
            return
        }
        detach()
        source = got
        let asset = got.headers.isEmpty ? AVURLAsset(url: got.url) : AVURLAsset(url: got.url, options: ["AVURLAssetHTTPHeaderFieldsKey": got.headers])
        let item = AVPlayerItem(asset: asset)
        let player = AVPlayer(playerItem: item)
        playersBuilt += 1
        player.isMuted = muted
        timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(seconds: 0.25, preferredTimescale: 600), queue: .main) { [weak self] t in
            MainActor.assumeIsolated { self?.tick(t) }
        }
        endObserver = NotificationCenter.default.addObserver(forName: .AVPlayerItemDidPlayToEndTime, object: item, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.playing = false
                self?.player?.seek(to: .zero)
            }
        }
        self.player = player
        player.play()
        playing = true
    }

    func pause() {
        player?.pause()
        playing = false
    }

    /// Seek to `fraction` (0...1) of the known length.
    func seek(fraction: Double, fallbackDuration: Double?) {
        guard let player, let total = duration ?? fallbackDuration, total > 0 else { return }
        let to = min(1, max(0, fraction)) * total
        time = to
        player.seek(to: CMTime(seconds: to, preferredTimescale: 600))
    }

    /// Drop the player and any open in flight (the leaf left the screen or
    /// its src changed); a fetched file no other player uses is deleted.
    func stop() {
        opening?.task.cancel()
        opening = nil
        generation += 1
        detach()
        openedKey = nil
        loading = false
        playing = false
        time = 0
        duration = nil
    }

    /// The current player's observers removed BEFORE it goes, its source
    /// released.
    private func detach() {
        if let timeObserver { player?.removeTimeObserver(timeObserver) }
        if let endObserver { NotificationCenter.default.removeObserver(endObserver) }
        timeObserver = nil
        endObserver = nil
        player?.pause()
        player?.replaceCurrentItem(with: nil)
        player = nil
        MediaLoader.shared.release(source)
        source = nil
    }

    /// The observers attached to the current player (tests).
    var observing: Bool { timeObserver != nil || endObserver != nil }

    private func tick(_ t: CMTime) {
        time = t.seconds.isFinite ? t.seconds : 0
        if let d = player?.currentItem?.duration.seconds, d.isFinite, d > 0 { duration = d }
        playing = (player?.rate ?? 0) != 0
    }
}
