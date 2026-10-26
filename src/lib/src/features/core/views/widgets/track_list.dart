import 'package:material_ui/material_ui.dart';

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

  const AppTrackList.builder({
    required this.itemCount,
    required this.itemBuilder,
    this.padding = EdgeInsets.zero,
    this.margin = EdgeInsets.zero,
    this.listPadding = EdgeInsets.zero,
    this.physics,
    this.addAutomaticKeepAlives = true,
    this.cacheExtent,
    super.key,
  });

  @override
  Widget build(BuildContext context) => Padding(
    padding: margin,
    child: ListView.builder(
      physics: physics,
      itemCount: itemCount,
      padding: listPadding,
      cacheExtent: cacheExtent,
      addAutomaticKeepAlives: addAutomaticKeepAlives,
      itemBuilder: (context, index) => Padding(
        padding: padding.add(
          EdgeInsets.only(bottom: index + 1 < itemCount ? 3 : 0),
        ),
        child: itemBuilder(context, index),
      ),
    ),
  );
}

class AppSliverTrackList extends StatelessWidget {
  final int itemCount;
  final IndexedWidgetBuilder itemBuilder;
  final EdgeInsetsGeometry padding;
  final bool addAutomaticKeepAlives;

  const AppSliverTrackList({
    required this.itemCount,
    required this.itemBuilder,
    this.padding = EdgeInsets.zero,
    this.addAutomaticKeepAlives = true,
    super.key,
  });

  @override
  Widget build(BuildContext context) => SliverList(
    delegate: SliverChildBuilderDelegate(
      (context, index) => Padding(
        padding: padding.add(
          EdgeInsets.only(bottom: index + 1 < itemCount ? 3 : 0),
        ),
        child: itemBuilder(context, index),
      ),
      childCount: itemCount,
      addAutomaticKeepAlives: addAutomaticKeepAlives,
    ),
  );
}
