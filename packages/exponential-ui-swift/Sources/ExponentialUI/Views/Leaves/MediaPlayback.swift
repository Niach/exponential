import AVFoundation
import Observation

/// One Video / AudioPlayer's player (VAPP-103): its `src` plays ONLY through
/// the surface's policed media request (`SurfaceModel.mediaRequest`: the
/// host's resolveUrl, then the media policy); a denied src never reaches a
/// player and the leaf stays the static poster / controls. Nothing loads
/// before playback starts (a press, or `autoplay`); a request with headers
/// is fetched under `media.limits` first (`MediaLoader.playableURL`).
@MainActor
@Observable
final class MediaPlayback {
    /// The player once a policed source opened (nil = not started, denied
    /// or failed).
    private(set) var player: AVPlayer?
    private(set) var playing = false
    /// Seconds played and the item's length (nil until it is known).
    private(set) var time: Double = 0
    private(set) var duration: Double?
    /// The request playback opened (tests).
    @ObservationIgnored private(set) var openedKey: String?
    @ObservationIgnored private var timeObserver: Any?
    @ObservationIgnored private var endObserver: NSObjectProtocol?

    /// Start playing `request` (muted for `autoplay`); a later request
    /// replaces the player. nil (a denied src) stops and loads nothing.
    func play(_ request: URLRequest?, muted: Bool = false) async {
        guard let request else { return stop() }
        let key = MediaLoader.key(request)
        if key == openedKey, let player {
            player.play()
            playing = true
            return
        }
        stop()
        openedKey = key
        guard let url = await MediaLoader.shared.playableURL(request), openedKey == key else { return }
        let item = AVPlayerItem(url: url)
        let player = AVPlayer(playerItem: item)
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

    /// Drop the player (the leaf left the screen or its src changed).
    func stop() {
        if let timeObserver { player?.removeTimeObserver(timeObserver) }
        if let endObserver { NotificationCenter.default.removeObserver(endObserver) }
        player?.pause()
        player = nil
        timeObserver = nil
        endObserver = nil
        openedKey = nil
        playing = false
        time = 0
        duration = nil
    }

    private func tick(_ t: CMTime) {
        time = t.seconds.isFinite ? t.seconds : 0
        if let d = player?.currentItem?.duration.seconds, d.isFinite, d > 0 { duration = d }
        playing = (player?.rate ?? 0) != 0
    }
}
