import 'package:material_ui/material_ui.dart';

class HorizontalScrollFade extends StatefulWidget {
  final Widget child;
  const HorizontalScrollFade({required this.child, super.key});
  @override
  State<HorizontalScrollFade> createState() => _HorizontalScrollFadeState();
}

class _HorizontalScrollFadeState extends State<HorizontalScrollFade> {
  bool _before = false;
  bool _after = false;
  void _update(ScrollMetrics metrics) {
    if (metrics.axis != Axis.horizontal) return;
    final before = metrics.extentBefore > 1;
    final after = metrics.extentAfter > 1;
    if (_before != before || _after != after) {
      setState(() {
        _before = before;
        _after = after;
      });
    }
  }

  @override
  Widget build(BuildContext context) =>
      NotificationListener<ScrollMetricsNotification>(
        onNotification: (notification) {
          _update(notification.metrics);
          return false;
        },
        child: NotificationListener<ScrollNotification>(
          onNotification: (notification) {
            _update(notification.metrics);
            return false;
          },
          child: ShaderMask(
            blendMode: BlendMode.dstIn,
            shaderCallback: (rect) {
              final edge = (20 / rect.width.clamp(1, double.infinity)).clamp(
                0.0,
                0.4,
              );
              return LinearGradient(
                colors: [
                  if (_before) Colors.transparent else Colors.white,
                  Colors.white,
                  Colors.white,
                  if (_after) Colors.transparent else Colors.white,
                ],
                stops: [0, edge, 1 - edge, 1],
              ).createShader(rect);
            },
            child: widget.child,
          ),
        ),
      );
}
