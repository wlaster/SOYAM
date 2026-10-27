import 'dart:ui' as ui;

import 'package:material_ui/material_ui.dart';

/// Keeps the selected button at its original height while folding the rail,
/// then moves that button beyond the left edge. Reverses on edge hover.
class SidebarCollapse extends StatefulWidget {
  const SidebarCollapse({
    required this.visible,
    required this.selectedKey,
    required this.selectedChild,
    required this.color,
    required this.child,
    super.key,
  });

  final bool visible;
  final GlobalKey selectedKey;
  final Widget selectedChild;
  final Color color;
  final Widget child;

  @override
  State<SidebarCollapse> createState() => _SidebarCollapseState();
}

class _SidebarCollapseState extends State<SidebarCollapse>
    with SingleTickerProviderStateMixin {
  final GlobalKey _panelKey = GlobalKey();
  late final _motion = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 720),
    reverseDuration: const Duration(milliseconds: 420),
    value: widget.visible ? 0 : 1,
  );

  @override
  void didUpdateWidget(SidebarCollapse oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.visible != oldWidget.visible) {
      if (widget.visible) {
        _motion.reverse();
      } else {
        _motion.forward();
      }
    }
  }

  @override
  void dispose() {
    _motion.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _motion,
    builder: (context, _) {
      final fold = Curves.easeInOutCubic.transform(
        (_motion.value / 0.65).clamp(0.0, 1.0),
      );
      final slide = Curves.easeInOutCubic.transform(
        ((_motion.value - 0.65) / 0.35).clamp(0.0, 1.0),
      );
      return IgnorePointer(
        ignoring: !widget.visible,
        child: Transform.translate(
          key: const ValueKey('sidebar-translation'),
          offset: Offset(-140 * slide, 0),
          child: Stack(
            children: [
              Positioned.fill(
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final height = constraints.maxHeight;
                    var center = height / 2;
                    final selected = widget.selectedKey.currentContext
                        ?.findRenderObject();
                    final panel = _panelKey.currentContext?.findRenderObject();
                    if (selected is RenderBox &&
                        panel is RenderBox &&
                        selected.hasSize &&
                        panel.hasSize) {
                      center = selected
                          .localToGlobal(
                            Offset(0, selected.size.height / 2),
                            ancestor: panel,
                          )
                          .dy;
                    }
                    const collapsedHeight = 72.0;
                    final top = (center - collapsedHeight / 2) * fold;
                    final foldedHeight = ui.lerpDouble(
                      height,
                      collapsedHeight,
                      fold,
                    )!;
                    return Stack(
                      clipBehavior: Clip.none,
                      children: [
                        Positioned(
                          top: top,
                          left: 0,
                          right: 0,
                          height: foldedHeight,
                          child: Opacity(
                            opacity: fold.clamp(0.0, 1.0),
                            child: Container(
                              key: const ValueKey('sidebar-background'),
                              decoration: BoxDecoration(
                                color: widget.color,
                                borderRadius: BorderRadius.circular(32),
                                border: Border.all(
                                  color: Theme.of(context).colorScheme.onSurface
                                      .withValues(alpha: 0.1),
                                ),
                              ),
                            ),
                          ),
                        ),
                        Positioned(
                          top: center - 32,
                          left: 0,
                          right: 0,
                          height: 64,
                          child: IgnorePointer(
                            child: Opacity(
                              opacity: fold.clamp(0.0, 1.0),
                              child: Center(child: widget.selectedChild),
                            ),
                          ),
                        ),
                      ],
                    );
                  },
                ),
              ),
              Opacity(
                opacity: (1 - fold).clamp(0.0, 1.0),
                child: KeyedSubtree(key: _panelKey, child: widget.child),
              ),
            ],
          ),
        ),
      );
    },
  );
}
