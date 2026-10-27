import 'dart:async';
import 'dart:io';

import 'package:material_ui/material_ui.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/features/core/theme/app_tokens.dart';
import 'package:yayma/src/features/core/views/widgets/app_cover.dart';
import 'package:yayma/src/features/core/views/widgets/hover_scale.dart';
import 'package:yayma/src/features/core/views/widgets/tab_visibility.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';

class HomeCoverWidget extends StatefulWidget {
  const HomeCoverWidget({super.key});

  @override
  State<HomeCoverWidget> createState() => _HomeCoverWidgetState();
}

class _HomeCoverWidgetState extends State<HomeCoverWidget> {
  final ValueNotifier<bool> _hovered = ValueNotifier(false);

  @override
  void dispose() {
    _hovered.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SignalBuilder(
      builder: (context) {
        // No subscriptions while the tab is hidden.
        final meta = visibleSignal(context, trackMetadataSignal);
        final showLyrics = visibleSignal(context, showLyricsSignal);
        final isPlaying = visibleSignal(context, isPlayingSignal);
        final height = MediaQuery.of(context).size.height;
        final width = MediaQuery.of(context).size.width;
        final isNarrow = width < 600;

        var size = showLyrics ? 320.0 : 420.0;
        if (height < 800) size = showLyrics ? 280.0 : 360.0;
        if (height < 650) size = showLyrics ? 200.0 : 260.0;
        if (isNarrow && !showLyrics) size = width * 0.8;

        // The mobile cover keeps the motion of the queue carousel it replaced.
        final isMobile = Platform.isAndroid;
        final coverDuration = isMobile
            ? AppCoverMotion.duration
            : const Duration(milliseconds: 400);
        final coverCurve = isMobile ? AppCoverMotion.curve : Curves.easeOutBack;

        return HoverScale(
          hoverScale: 1,
          duration: const Duration(milliseconds: 150),
          curve: Curves.easeOut,
          onHoverChanged: (hovered) => _hovered.value = hovered,
          cursor: meta.id != null
              ? SystemMouseCursors.click
              : SystemMouseCursors.basic,
          onTap: meta.id != null
              ? () => unawaited(
                  PlaybackController.startTrackWave(meta.id!, meta.title),
                )
              : null,
          child: AnimatedScale(
            scale: isPlaying ? 1.0 : AppCoverMotion.largePausedScale,
            duration: coverDuration,
            curve: coverCurve,
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 350),
              curve: Curves.easeInOutCubic,
              width: size,
              height: size,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(AppRadius.md),
              ),
              child: AnimatedSwitcher(
                duration: const Duration(milliseconds: 500),
                transitionBuilder: (child, animation) {
                  return FadeTransition(
                    opacity: animation,
                    child: ScaleTransition(
                      scale: Tween<double>(
                        begin: 0.9,
                        end: 1,
                      ).animate(animation),
                      child: child,
                    ),
                  );
                },
                child: Stack(
                  key: ValueKey(meta.coverUrl),
                  children: [
                    AppCover(
                      coverUrl: meta.coverUrl,
                      size: size,
                      radius: AppRadius.md,
                    ),
                    if (meta.id != null)
                      Positioned.fill(
                        child: ValueListenableBuilder<bool>(
                          valueListenable: _hovered,
                          builder: (context, hovered, _) => AnimatedOpacity(
                            key: const ValueKey('home-cover-wave-hover'),
                            opacity: hovered ? 1 : 0,
                            duration: const Duration(milliseconds: 150),
                            child: Container(
                              decoration: BoxDecoration(
                                color: Colors.black.withValues(alpha: 0.45),
                                borderRadius: BorderRadius.circular(
                                  AppRadius.md,
                                ),
                              ),
                              alignment: Alignment.center,
                              child: const Column(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Icon(
                                    Icons.waves_rounded,
                                    size: 40,
                                    color: Colors.white,
                                  ),
                                  SizedBox(height: 10),
                                  Text(
                                    'Волна по треку',
                                    style: TextStyle(
                                      color: Colors.white,
                                      fontSize: 16,
                                      fontWeight: FontWeight.w600,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}
