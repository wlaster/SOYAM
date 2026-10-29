import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/theme/app_tokens.dart';

class AppContextMenuItem<T> {
  final T? value;
  final String label;
  final IconData? icon;
  final Widget? leading;
  final Color? color;
  final List<AppContextMenuItem<T>>? subItems;
  final bool isSelected;
  final bool searchable;
  final bool pinned;

  const AppContextMenuItem({
    required this.label,
    this.icon,
    this.leading,
    this.value,
    this.color,
    this.subItems,
    this.isSelected = false,
    this.searchable = false,
    this.pinned = false,
  });
}

class AppContextMenu<T> extends StatelessWidget {
  final List<AppContextMenuItem<T>> items;
  final void Function(T value) onSelected;
  final Widget child;
  final Widget Function(VoidCallback toggle)? triggerBuilder;
  final VoidCallback? onOpen;
  final VoidCallback? onClose;
  final BorderRadius borderRadius;

  /// Hover/press backdrop color; null keeps the theme default (nearly
  /// invisible). Icon triggers pass an M3-style state layer to match
  /// neighbouring IconButtons.
  final Color? hoverColor;

  const AppContextMenu({
    required this.items,
    required this.onSelected,
    required this.child,
    this.triggerBuilder,
    this.onOpen,
    this.onClose,
    this.borderRadius = const BorderRadius.all(Radius.circular(100)),
    this.hoverColor,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    return MenuAnchor(
      onOpen: onOpen,
      onClose: onClose,
      style: MenuStyle(
        backgroundColor: WidgetStateProperty.all(
          Theme.of(context).colorScheme.surfaceContainerHighest,
        ),
        surfaceTintColor: WidgetStateProperty.all(Colors.transparent),
        padding: WidgetStateProperty.all(EdgeInsets.zero),
        elevation: WidgetStateProperty.all(12),
        shape: WidgetStateProperty.all(
          RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(AppRadius.sm),
            side: BorderSide(
              color: Theme.of(
                context,
              ).colorScheme.onSurface.withValues(alpha: 0.1),
            ),
          ),
        ),
      ),
      menuChildren: [
        Builder(
          builder: (menuContext) => Column(
            key: const ValueKey('context-menu-body'),
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: items
                .asMap()
                .entries
                .map((entry) => _buildItem(menuContext, entry.value, entry.key))
                .toList(),
          ),
        ),
      ],
      builder: (context, controller, child) {
        void toggle() =>
            controller.isOpen ? controller.close() : controller.open();
        if (triggerBuilder != null) return triggerBuilder!(toggle);
        // Local Material: ink paints at the trigger itself instead of a
        // Material buried under opaque backgrounds (e.g. the player backdrop).
        return Material(
          color: Colors.transparent,
          child: InkWell(
            onTap: () {
              if (controller.isOpen) {
                controller.close();
              } else {
                controller.open();
              }
            },
            hoverColor: hoverColor,
            highlightColor: hoverColor,
            borderRadius: borderRadius,
            child: this.child,
          ),
        );
      },
    );
  }

  Widget _buildItem(
    BuildContext context,
    AppContextMenuItem<T> item,
    int index,
  ) {
    final child = _buildItemContent(context, item, index);

    return TweenAnimationBuilder<double>(
      duration: Duration(milliseconds: 300 + (index * 50)),
      curve: Curves.easeOutQuart,
      tween: Tween(begin: 0, end: 1),
      builder: (context, value, child) {
        // Staggered effect: calculate a delayed value based on index
        final startDelay = (index * 0.1).clamp(0.0, 0.5);
        final effectiveValue = ((value - startDelay) / (1.0 - startDelay))
            .clamp(0.0, 1.0);

        return Opacity(
          opacity: effectiveValue,
          child: Transform.translate(
            offset: Offset(0, 8 * (1 - effectiveValue)),
            child: Transform.scale(
              scale: 0.95 + (0.05 * effectiveValue),
              child: child,
            ),
          ),
        );
      },
      child: child,
    );
  }

  Widget _buildItemContent(
    BuildContext context,
    AppContextMenuItem<T> item,
    int index,
  ) {
    final leading =
        item.leading ??
        (item.icon != null
            ? Icon(
                item.icon,
                color:
                    item.color ??
                    Theme.of(context).colorScheme.onSurfaceVariant,
                size: 18,
              )
            : null);

    if (item.subItems != null && item.subItems!.isNotEmpty) {
      return SubmenuButton(
        clipBehavior: Clip.antiAlias,
        menuStyle: MenuStyle(
          backgroundColor: WidgetStateProperty.all(
            Theme.of(context).colorScheme.surfaceContainerHighest,
          ),
          surfaceTintColor: WidgetStateProperty.all(Colors.transparent),
          padding: WidgetStateProperty.all(EdgeInsets.zero),
          shape: WidgetStateProperty.all(
            RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(AppRadius.sm),
              side: BorderSide(
                color: Theme.of(
                  context,
                ).colorScheme.onSurface.withValues(alpha: 0.1),
              ),
            ),
          ),
        ),
        menuChildren: item.searchable
            ? [
                _SearchableSubmenu<T>(
                  parentMenuContext: context,
                  items: item.subItems!,
                  itemBuilder: (item) => _buildItemContent(context, item, 0),
                ),
              ]
            : item.subItems!
                  .asMap()
                  .entries
                  .map((entry) => _buildItem(context, entry.value, entry.key))
                  .toList(),
        leadingIcon: leading,
        child: Text(
          item.label,
          style: TextStyle(
            color: item.color ?? Theme.of(context).colorScheme.onSurface,
            fontSize: 14,
          ),
        ),
      );
    }

    return MenuItemButton(
      style: ButtonStyle(
        shape: WidgetStatePropertyAll(
          RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
        ),
      ),
      onPressed: () {
        if (item.value != null) {
          onSelected(item.value as T);
        }
      },
      leadingIcon: leading,
      trailingIcon: item.isSelected
          ? Icon(
              Icons.check_rounded,
              color: item.color ?? Theme.of(context).colorScheme.primary,
              size: 16,
            )
          : null,
      child: Text(
        item.label,
        style: TextStyle(
          color: item.color ?? Theme.of(context).colorScheme.onSurface,
          fontSize: 14,
        ),
      ),
    );
  }
}

class _SearchableSubmenu<T> extends StatefulWidget {
  const _SearchableSubmenu({
    required this.items,
    required this.itemBuilder,
    required this.parentMenuContext,
  });
  final BuildContext parentMenuContext;
  final List<AppContextMenuItem<T>> items;
  final Widget Function(AppContextMenuItem<T>) itemBuilder;
  @override
  State<_SearchableSubmenu<T>> createState() => _SearchableSubmenuState<T>();
}

class _SearchableSubmenuState<T> extends State<_SearchableSubmenu<T>> {
  final _scroll = ScrollController();
  String _query = '';
  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final filtered = widget.items
        .where(
          (item) =>
              !item.pinned &&
              item.label.toLowerCase().contains(_query.trim().toLowerCase()),
        )
        .toList();
    final parent = widget.parentMenuContext.findRenderObject();
    final height = parent is RenderBox && parent.hasSize
        ? parent.size.height - 2
        : MediaQuery.sizeOf(context).height * 0.65;
    final desired =
        64.0 +
        widget.items.where((item) => item.pinned).length * 56 +
        (filtered.isEmpty ? 48 : filtered.length * 52 + 4);
    return SizedBox(
      key: const ValueKey('playlist-submenu-body'),
      width: 300,
      height: desired.clamp(
        120.0,
        height.clamp(120.0, MediaQuery.sizeOf(context).height),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: TextField(
              key: const ValueKey('playlist-menu-search'),
              decoration: const InputDecoration(
                hintText: 'Поиск плейлиста',
                prefixIcon: Icon(Icons.search_rounded),
                isDense: true,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.all(Radius.circular(12)),
                ),
              ),
              onChanged: (value) {
                setState(() => _query = value);
                if (_scroll.hasClients) _scroll.jumpTo(0);
              },
            ),
          ),
          Expanded(
            child: filtered.isEmpty
                ? const SizedBox(
                    height: 48,
                    child: Center(child: Text('Плейлисты не найдены')),
                  )
                : ClipRRect(
                    borderRadius: BorderRadius.circular(12),
                    child: Scrollbar(
                      controller: _scroll,
                      thumbVisibility: true,
                      child: ScrollConfiguration(
                        behavior: ScrollConfiguration.of(
                          context,
                        ).copyWith(scrollbars: false),
                        child: ListView.separated(
                          controller: _scroll,
                          padding: const EdgeInsets.symmetric(
                            horizontal: 4,
                            vertical: 4,
                          ),
                          itemCount: filtered.length,
                          separatorBuilder: (_, _) => const SizedBox(height: 4),
                          itemBuilder: (context, index) =>
                              widget.itemBuilder(filtered[index]),
                        ),
                      ),
                    ),
                  ),
          ),
          for (final item in widget.items.where((item) => item.pinned))
            Padding(
              padding: const EdgeInsets.all(4),
              child: widget.itemBuilder(item),
            ),
        ],
      ),
    );
  }
}
