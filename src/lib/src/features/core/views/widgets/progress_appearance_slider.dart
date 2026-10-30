import 'dart:math' as math;
import 'package:material_ui/material_ui.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/features/core/providers/visual_effects_provider.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';

class ProgressAppearanceSlider extends StatefulWidget {
  const ProgressAppearanceSlider({
    required this.appearance,
    required this.value,
    required this.max,
    required this.color,
    required this.inactiveColor,
    required this.onStart,
    required this.onChanged,
    required this.onEnd,
    super.key,
  });
  final ProgressAppearance appearance;
  final double value;
  final double max;
  final Color color;
  final Color inactiveColor;
  final ValueChanged<double> onStart;
  final ValueChanged<double> onChanged;
  final ValueChanged<double> onEnd;
  @override
  State<ProgressAppearanceSlider> createState() =>
      _ProgressAppearanceSliderState();
}

class _ProgressAppearanceSliderState extends State<ProgressAppearanceSlider>
    with TickerProviderStateMixin {
  late final AnimationController _phase = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 3200),
  );
  late final AnimationController _amplitude = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 200),
    value: isPlayingSignal.peek() ? 1 : 0,
  );
  bool? _wavePlaying;
  late final EffectCleanup _playingSubscription;

  void _updateMotion() {
    final playing =
        widget.appearance == ProgressAppearance.android &&
        isPlayingSignal.peek();
    if (_wavePlaying != playing) {
      _wavePlaying = playing;
      _amplitude.animateTo(playing ? 1 : 0, curve: Curves.easeOutCubic);
    }
    if (playing) {
      if (!_phase.isAnimating) _phase.repeat();
    } else {
      _phase.stop();
    }
  }

  @override
  void initState() {
    super.initState();
    _playingSubscription = isPlayingSignal.subscribe((_) => _updateMotion());
    _updateMotion();
  }

  @override
  void didUpdateWidget(covariant ProgressAppearanceSlider oldWidget) {
    super.didUpdateWidget(oldWidget);
    _updateMotion();
  }

  @override
  void dispose() {
    _playingSubscription();
    _phase.dispose();
    _amplitude.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final appearance = widget.appearance;
    final value = widget.value;
    final max = widget.max;
    final color = widget.color;
    final inactiveColor = widget.inactiveColor;
    final onStart = widget.onStart;
    final onChanged = widget.onChanged;
    final onEnd = widget.onEnd;
    return SizedBox(
      height: 28,
      child: Stack(
        children: [
          Positioned.fill(
            child: IgnorePointer(
              child: CustomPaint(
                painter: _ProgressPainter(
                  appearance,
                  value / max,
                  color,
                  inactiveColor,
                  MediaQuery.devicePixelRatioOf(context),
                  _phase,
                  _amplitude,
                ),
              ),
            ),
          ),
          Positioned.fill(
            child: SliderTheme(
              data: SliderTheme.of(context).copyWith(
                trackHeight: 0,
                activeTrackColor: Colors.transparent,
                inactiveTrackColor: Colors.transparent,
                thumbColor: Colors.transparent,
                overlayColor: Colors.transparent,
                thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 0),
                overlayShape: const RoundSliderOverlayShape(overlayRadius: 0),
                showValueIndicator: ShowValueIndicator.never,
              ),
              child: Slider(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                value: value,
                max: max,
                onChangeStart: onStart,
                onChanged: onChanged,
                onChangeEnd: onEnd,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ProgressPainter extends CustomPainter {
  _ProgressPainter(
    this.appearance,
    this.fraction,
    this.color,
    this.inactive,
    this.dpr,
    this.phase,
    this.amplitude,
  ) : super(
        repaint: appearance == ProgressAppearance.visualizer
            ? vibeTickSignal
            : appearance == ProgressAppearance.android
            ? Listenable.merge([phase, amplitude])
            : null,
      );
  final ProgressAppearance appearance;
  final double fraction;
  final Color color;
  final Color inactive;
  final double dpr;
  final Animation<double> phase;
  final Animation<double> amplitude;
  @override
  void paint(Canvas canvas, Size size) {
    const left = 16.0;
    final width = math.max(0, size.width - 32);
    final end = left + width * fraction.clamp(0, 1);
    final y = size.height / 2;
    final paint = Paint()
      ..isAntiAlias = true
      ..strokeCap = StrokeCap.round
      ..strokeWidth = 4.5;
    if (appearance == ProgressAppearance.android) {
      canvas.drawLine(
        Offset(end, y),
        Offset(left + width, y),
        paint..color = inactive,
      );
      final activeWidth = end - left;
      final amplitude =
          3.5 * (activeWidth / 10).clamp(0.0, 1.0) * this.amplitude.value;
      double waveY(double x) {
        final distance = ((end - x) / 10).clamp(0.0, 1.0);
        final envelope = distance * distance * (3 - 2 * distance);
        return y +
            math.sin((x - end) / 5 + phase.value * 2 * math.pi) *
                amplitude *
                envelope;
      }

      final path = Path()..moveTo(left, waveY(left));
      for (var x = left; x < end; x += 4) {
        final next = math.min(x + 4, end);
        final step = (next - x) / 3;
        double slope(double at) => (waveY(at + 0.01) - waveY(at - 0.01)) / 0.02;
        path.cubicTo(
          x + step,
          waveY(x) + slope(x) * step,
          next - step,
          waveY(next) - slope(next) * step,
          next,
          waveY(next),
        );
      }
      path.lineTo(end, y);
      canvas.drawPath(
        path,
        paint
          ..color = color
          ..style = PaintingStyle.stroke,
      );
      paint.style = PaintingStyle.fill;
      canvas.drawCircle(Offset(end, y), 6, paint);
    } else {
      final bands = vibeTickSignal.peek();
      final count = math.max(1, (width / 5).floor());
      for (var i = 0; i < count; i++) {
        final rawX = left + width * (i + 0.5) / count;
        final x = (rawX * dpr).roundToDouble() / dpr;
        final energy = bands.length > 4
            ? bands[2 + i % 3].clamp(0.0, 1.0)
            : 0.0;
        final height =
            3 + (3 + energy * 8) * (0.35 + 0.65 * math.sin(i * 1.7).abs());
        canvas.drawLine(
          Offset(x, y - height / 2),
          Offset(x, y + height / 2),
          paint
            ..color = x <= end ? color : inactive
            ..strokeWidth = 2,
        );
      }
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          Rect.fromCenter(center: Offset(end, y), width: 3, height: 20),
          const Radius.circular(2),
        ),
        Paint()..color = color,
      );
    }
  }

  @override
  bool shouldRepaint(_ProgressPainter old) =>
      old.appearance != appearance ||
      old.fraction != fraction ||
      old.color != color ||
      old.inactive != inactive ||
      old.dpr != dpr;
}
