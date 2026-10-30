import 'package:flutter/rendering.dart' show ScrollCacheExtent;
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';

/// Rows own their interactions. An extra focusable segmented card can change
/// the child tree when a menu gains focus, disposing the open MenuAnchor.
class AppTrackList extends StatelessWidget {
  final int itemCount;
  final IndexedWidgetBuilder itemBuilder;
  final EdgeInsetsGeometry padding;
  final EdgeInsetsGeometry margin;
  final EdgeInsetsGeometry listPadding;
  final ScrollPhysics? physics;
  final bool addAutomaticKeepAlives;
  final double? cacheExtent;
  final Set<int> unavailableIndices;

  const AppTrackList.builder({
    required this.itemCount,
    required this.itemBuilder,
    this.padding = EdgeInsets.zero,
    this.margin = EdgeInsets.zero,
    this.listPadding = EdgeInsets.zero,
    this.physics,
    this.addAutomaticKeepAlives = true,
    this.cacheExtent,
    this.unavailableIndices = const {},
    super.key,
  });

  @override
  Widget build(BuildContext context) => Padding(
    padding: margin,
    child: unavailableIndices.isNotEmpty
        ? CustomScrollView(
            physics: physics,
            scrollCacheExtent: cacheExtent == null
                ? null
                : ScrollCacheExtent.pixels(cacheExtent!),
            slivers: [
              AppSliverTrackList(
                itemCount: itemCount,
                itemBuilder: itemBuilder,
                padding: padding,
                addAutomaticKeepAlives: addAutomaticKeepAlives,
                unavailableIndices: unavailableIndices,
              ),
              SliverPadding(padding: listPadding),
            ],
          )
        : ListView.builder(
            physics: physics,
            itemCount: itemCount,
            padding: listPadding,
            scrollCacheExtent: cacheExtent == null
                ? null
                : ScrollCacheExtent.pixels(cacheExtent!),
            addAutomaticKeepAlives: addAutomaticKeepAlives,
            itemBuilder: (context, index) =>
                _trackListItem(context, index, itemCount, padding, itemBuilder),
          ),
  );
}

class AppSliverTrackList extends StatefulWidget {
  final int itemCount;
  final IndexedWidgetBuilder itemBuilder;
  final EdgeInsetsGeometry padding;
  final bool addAutomaticKeepAlives;
  final Set<int> unavailableIndices;
  final Widget? availableSliver;
  final Widget? availableHeader;

  const AppSliverTrackList({
    required this.itemCount,
    required this.itemBuilder,
    this.padding = EdgeInsets.zero,
    this.addAutomaticKeepAlives = true,
    this.unavailableIndices = const {},
    this.availableSliver,
    this.availableHeader,
    super.key,
  });

  @override
  State<AppSliverTrackList> createState() => _AppSliverTrackListState();
}

class _AppSliverTrackListState extends State<AppSliverTrackList>
    with SingleTickerProviderStateMixin {
  bool _expanded = true;
  late final AnimationController _motion = AnimationController(
    vsync: this,
    value: 1,
    duration: const Duration(milliseconds: 240),
  );
  late final Animation<double> _size = _motion.drive(
    CurveTween(curve: Curves.easeInOutCubic),
  );

  void _toggle() {
    setState(() => _expanded = !_expanded);
    if (MediaQuery.disableAnimationsOf(context)) {
      _motion.value = _expanded ? 1 : 0;
    } else if (_expanded) {
      _motion.forward();
    } else {
      _motion.reverse();
    }
  }

  @override
  void dispose() {
    _motion.dispose();
    super.dispose();
  }

  Widget _list(List<int> indices, {bool animated = false}) => SliverList(
    delegate: SliverChildBuilderDelegate(
      (context, index) {
        final child = _trackListItem(
          context,
          index,
          indices.length,
          widget.padding,
          (context, _) => widget.itemBuilder(context, indices[index]),
        );
        return animated
            ? SizeTransition(
                sizeFactor: _size,
                alignment: Alignment.topCenter,
                child: FadeTransition(opacity: _size, child: child),
              )
            : child;
      },
      childCount: indices.length,
      addAutomaticKeepAlives: widget.addAutomaticKeepAlives,
    ),
  );

  @override
  Widget build(BuildContext context) {
    final available = <int>[];
    final unavailable = <int>[];
    for (var i = 0; i < widget.itemCount; i++) {
      (widget.unavailableIndices.contains(i) ? unavailable : available).add(i);
    }
    return SliverMainAxisGroup(
      slivers: [
        if (unavailable.isNotEmpty) ...[
          SliverToBoxAdapter(
            child: Padding(
              padding: EdgeInsets.fromLTRB(
                widget.padding.resolve(Directionality.of(context)).left,
                24,
                widget.padding.resolve(Directionality.of(context)).right,
                16,
              ),
              child: Row(
                children: [
                  const Flexible(
                    child: CommonSectionTitle(
                      title: 'Недоступные треки',
                      padding: EdgeInsets.zero,
                    ),
                  ),
                  const SizedBox(width: 8),
                  IconButton(
                    tooltip: _expanded ? 'Свернуть' : 'Развернуть',
                    onPressed: _toggle,
                    style: const ButtonStyle(
                      shape: WidgetStatePropertyAll(
                        RoundedRectangleBorder(
                          borderRadius: BorderRadius.all(Radius.circular(12)),
                        ),
                      ),
                    ),
                    icon: AnimatedRotation(
                      turns: _expanded ? 0 : .5,
                      duration: MediaQuery.disableAnimationsOf(context)
                          ? Duration.zero
                          : const Duration(milliseconds: 200),
                      child: const Icon(Icons.expand_less_rounded),
                    ),
                  ),
                ],
              ),
            ),
          ),
          AnimatedBuilder(
            animation: _motion,
            builder: (context, _) => SliverIgnorePointer(
              ignoring: !_expanded,
              sliver: _motion.value == 0
                  ? const SliverToBoxAdapter(child: SizedBox.shrink())
                  : _list(unavailable, animated: true),
            ),
          ),
        ],
        if (available.isNotEmpty) ...[
          if (widget.availableHeader != null)
            SliverToBoxAdapter(child: widget.availableHeader),
          widget.availableSliver ?? _list(available),
        ],
      ],
    );
  }
}

Widget _trackListItem(
  BuildContext context,
  int index,
  int count,
  EdgeInsetsGeometry padding,
  IndexedWidgetBuilder builder,
) => Padding(
  padding: padding.add(EdgeInsets.only(bottom: index + 1 < count ? 3 : 0)),
  child: builder(context, index),
);
