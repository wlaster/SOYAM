import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/app_context_menu.dart';

void main() {
  testWidgets(
    'playlist submenu scrolls, searches and selects; create remains pinned',
    (tester) async {
      String? selected;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: AppContextMenu<String>(
                onSelected: (value) => selected = value,
                items: [
                  AppContextMenuItem(
                    label: 'Add',
                    searchable: true,
                    subItems: [
                      for (var i = 0; i < 30; i++)
                        AppContextMenuItem(value: '$i', label: 'Mix $i'),
                      const AppContextMenuItem(
                        value: 'create',
                        label: 'Create',
                        pinned: true,
                      ),
                    ],
                  ),
                  for (var i = 0; i < 7; i++)
                    AppContextMenuItem(value: 'other$i', label: 'Other $i'),
                ],
                child: const Text('Open'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add'));
      await tester.pumpAndSettle();
      final parentRect = tester.getRect(
        find.byKey(const ValueKey('context-menu-body')),
      );
      final submenu = find.byKey(const ValueKey('playlist-submenu-body'));
      final expandedRect = tester.getRect(submenu);
      expect(expandedRect.top, closeTo(parentRect.top, 2));
      expect(expandedRect.bottom, lessThanOrEqualTo(parentRect.bottom + 2));
      expect(
        tester
            .widgetList<Scrollbar>(find.byType(Scrollbar))
            .any((bar) => bar.thumbVisibility == true),
        isTrue,
      );
      expect(find.text('Create'), findsOneWidget);
      await tester.scrollUntilVisible(
        find.text('Mix 29'),
        200,
        scrollable: find.descendant(
          of: find.byType(ListView),
          matching: find.byType(Scrollable),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Mix 29'), findsOneWidget);
      await tester.enterText(
        find.byKey(const ValueKey('playlist-menu-search')),
        'Mix 29',
      );
      await tester.pumpAndSettle();
      expect(find.text('Mix 0'), findsNothing);
      expect(tester.getSize(submenu).height, lessThan(expandedRect.height));
      expect(tester.getRect(submenu).top, closeTo(parentRect.top, 2));
      expect(find.text('Create'), findsOneWidget);
      await tester.tap(find.widgetWithText(MenuItemButton, 'Mix 29'));
      await tester.pumpAndSettle();
      expect(selected, '29');
      expect(tester.takeException(), isNull);
    },
  );
}
