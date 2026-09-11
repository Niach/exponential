import AVFoundation
import AVKit
import ExpCore
import ExpUI
import SwiftUI
import UIKit

// EXP-824 — the inline players every media surface shares: the description
// and comment bodies (`BlockMediaView`), the posted-comment strip and the
// agent transcript. A video shows its poster inside an aspect box with a play
// glyph and a duration chip, plays inline on tap (AVPlayer) and goes
// fullscreen through the native player (`MediaAssets.presentFullscreen`); an
// audio row is play/pause + progress + duration.

/// The duration chip: `0:07` on a dark pill, bottom-trailing on the poster.
struct MediaDurationChip: View {
    let durationMs: Int

    var body: some View {
        Text(MediaDuration.format(ms: durationMs))
            .font(.caption2.weight(.medium).monospacedDigit())
            .foregroundStyle(.white)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(.black.opacity(0.6), in: Capsule())
    }
}

/// The centered play glyph on a poster.
struct MediaPlayGlyph: View {
    var size: CGFloat = 52

    var body: some View {
        Image(systemName: "play.fill")
            .font(.system(size: size * 0.42, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(.black.opacity(0.55), in: Circle())
            .overlay(Circle().stroke(.white.opacity(0.25), lineWidth: 0.5))
            .accessibilityHidden(true)
    }
}

/// A video's poster frame with the play glyph and duration chip over it —
/// the placeholder the player shows before the first tap and the tile the
/// comment strip shows. `poster` nil paints a neutral box in the aspect
/// ratio; `dimmed` tones the glyph down while an upload is in flight.
struct VideoPosterTile: View {
    let poster: UIImage?
    let aspectRatio: CGFloat
    let durationMs: Int?
    var dimmed = false

    var body: some View {
        ZStack {
            if let poster {
                Image(uiImage: poster)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                Color.white.opacity(0.06)
            }
            MediaPlayGlyph()
                .opacity(dimmed ? 0.35 : 1)
        }
        .aspectRatio(aspectRatio, contentMode: .fit)
        .overlay(alignment: .bottomTrailing) {
            if let durationMs, durationMs > 0 {
                MediaDurationChip(durationMs: durationMs)
                    .padding(8)
            }
        }
    }
}

/// Inline video: poster → tap → `VideoPlayer` in the same box, plus a
/// fullscreen button that hands the running `AVPlayer` to the system player.
struct InlineVideoPlayerView: View {
    let attachmentId: String
    let url: String
    let info: AttachmentMediaInfo
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    /// Height cap for compact contexts (the comment composer, chat bubbles).
    var maxHeight: CGFloat?

    @Environment(AppDependencies.self) private var deps
    @State private var poster: UIImage?
    @State private var player: AVPlayer?
    @State private var failed = false

    private var aspectRatio: CGFloat { CGFloat(info.aspectRatio) }

    var body: some View {
        ZStack {
            if let player {
                VideoPlayer(player: player)
                    .aspectRatio(aspectRatio, contentMode: .fit)
                    .overlay(alignment: .topTrailing) {
                        Button {
                            MediaAssets.presentFullscreen(player: player)
                        } label: {
                            AppIcon(AppIcons.uiFullscreen, size: 14, weight: .semibold)
                                .foregroundStyle(.white)
                                .frame(width: 30, height: 30)
                                .background(.black.opacity(0.55), in: Circle())
                        }
                        .buttonStyle(.plain)
                        .padding(8)
                        .accessibilityLabel("Fullscreen")
                    }
            } else {
                Button {
                    play()
                } label: {
                    VideoPosterTile(poster: poster, aspectRatio: aspectRatio, durationMs: info.durationMs)
                        .overlay(alignment: .bottomLeading) {
                            if failed {
                                Text("Couldn't play this video")
                                    .font(.caption)
                                    .foregroundStyle(.white)
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 6)
                                    .background(.black.opacity(0.45), in: Capsule())
                                    .padding(8)
                            }
                        }
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Play video")
            }
        }
        .frame(maxWidth: .infinity)
        .frame(maxHeight: maxHeight ?? .infinity, alignment: .leading)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .task(id: "\(attachmentId)|\(info.hasPoster)") { await loadPoster() }
        .onDisappear { player?.pause() }
    }

    private func play() {
        failed = false
        guard let asset = MediaAssets.makeAsset(
            urlString: url, baseURL: baseURL, auth: deps.auth, accountId: accountId
        ) else {
            failed = true
            return
        }
        let item = AVPlayerItem(asset: asset)
        let player = AVPlayer(playerItem: item)
        self.player = player
        player.play()
    }

    private func loadPoster() async {
        guard info.hasPoster, poster == nil else { return }
        let loader = AttachmentImageLoader(
            baseURL: baseURL, accountId: accountId, httpClient: httpClient, pendingImages: [:]
        )
        poster = try? await loader.load(AttachmentLinks.posterUrl(attachmentId: attachmentId))
    }
}

/// Inline audio: a row with play/pause, a scrubber and `elapsed / total`.
struct InlineAudioPlayerView: View {
    let url: String
    let label: String
    let info: AttachmentMediaInfo
    let baseURL: URL?
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @State private var controller = AudioPlayerController()

    private var totalMs: Int {
        controller.durationMs ?? info.durationMs ?? 0
    }

    var body: some View {
        HStack(spacing: 10) {
            Button {
                if controller.player == nil {
                    guard let asset = MediaAssets.makeAsset(
                        urlString: url, baseURL: baseURL, auth: deps.auth, accountId: accountId
                    ) else { return }
                    controller.load(asset: asset)
                }
                controller.toggle()
            } label: {
                Image(systemName: controller.isPlaying ? "pause.fill" : "play.fill")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 32, height: 32)
                    .background(Color.white.opacity(0.12), in: Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(controller.isPlaying ? "Pause" : "Play")

            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Image(systemName: "waveform")
                        .font(.system(size: 11))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Text(label)
                        .font(.caption)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Slider(
                    value: Binding(
                        get: { Double(controller.currentMs) },
                        set: { controller.seek(toMs: Int($0)) }
                    ),
                    in: 0...Double(max(totalMs, 1))
                )
                .tint(.white.opacity(0.8))
                .disabled(controller.player == nil)
            }

            Text("\(MediaDuration.format(ms: controller.currentMs)) / \(MediaDuration.format(ms: totalMs))")
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
                .fixedSize()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .glassRow()
        .onDisappear { controller.tearDown() }
    }
}

/// The AVPlayer behind an audio row, kept in `@State` so the periodic time
/// observer outlives view re-renders. Main-actor: every mutation lands on the
/// observable state SwiftUI reads.
@MainActor
@Observable
final class AudioPlayerController {
    private(set) var player: AVPlayer?
    private(set) var isPlaying = false
    private(set) var currentMs = 0
    private(set) var durationMs: Int?
    private var timeObserver: Any?
    private var endObserver: NSObjectProtocol?

    func load(asset: AVURLAsset) {
        let item = AVPlayerItem(asset: asset)
        let player = AVPlayer(playerItem: item)
        self.player = player
        timeObserver = player.addPeriodicTimeObserver(
            forInterval: CMTime(seconds: 0.25, preferredTimescale: 600),
            queue: .main
        ) { [weak self] time in
            // The observer's queue is main; hop through the actor explicitly
            // so the closure stays Sendable under strict concurrency.
            let seconds = time.seconds
            Task { @MainActor [weak self] in
                guard let self, seconds.isFinite else { return }
                self.currentMs = Int(seconds * 1000)
                if let duration = self.player?.currentItem?.duration.seconds, duration.isFinite, duration > 0 {
                    self.durationMs = Int(duration * 1000)
                }
            }
        }
        endObserver = NotificationCenter.default.addObserver(
            forName: .AVPlayerItemDidPlayToEndTime, object: item, queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.isPlaying = false
                self?.player?.seek(to: .zero)
                self?.currentMs = 0
            }
        }
    }

    func toggle() {
        guard let player else { return }
        if isPlaying {
            player.pause()
            isPlaying = false
        } else {
            player.play()
            isPlaying = true
        }
    }

    func pause() {
        player?.pause()
        isPlaying = false
    }

    func seek(toMs ms: Int) {
        currentMs = ms
        player?.seek(to: CMTime(seconds: Double(ms) / 1000, preferredTimescale: 600))
    }

    /// Release the player + observers. Called from the row's `onDisappear`
    /// rather than a `deinit`: a deinit is nonisolated under strict
    /// concurrency and may not touch this actor's state. The next tap
    /// rebuilds the player from the asset.
    func tearDown() {
        pause()
        if let timeObserver { player?.removeTimeObserver(timeObserver) }
        if let endObserver { NotificationCenter.default.removeObserver(endObserver) }
        timeObserver = nil
        endObserver = nil
        player = nil
        currentMs = 0
    }
}
