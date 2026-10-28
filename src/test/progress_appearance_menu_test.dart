import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/features/core/providers/visual_effects_provider.dart';
import 'package:yayma/src/features/settings/views/settings_view.dart';

void main() {
  testWidgets('progress menu updates its label and selected item', (
    tester,
  ) async {
    progressAppearanceSignal.value = ProgressAppearance.standard;
    addTearDown(
      () => progressAppearanceSignal.value = ProgressAppearance.standard,
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SignalBuilder(
            builder: (context) => SettingsProgressAppearanceMenu(
              selected: progressAppearanceSignal.value,
            ),
          ),
        ),
      ),
    );
    expect(find.widgetWithText(TextButton, 'Стандартный'), findsOneWidget);
    await tester.tap(find.byType(TextButton));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(MenuItemButton, 'Android'));
    await tester.pumpAndSettle();
    expect(progressAppearanceSignal.value, ProgressAppearance.android);
    expect(find.widgetWithText(TextButton, 'Android'), findsOneWidget);
    await tester.tap(find.byType(TextButton));
    await tester.pumpAndSettle();
    final android = tester.widget<MenuItemButton>(
      find.widgetWithText(MenuItemButton, 'Android'),
    );
    final standard = tester.widget<MenuItemButton>(
      find.widgetWithText(MenuItemButton, 'Стандартный'),
    );
    expect(
      android.style!.backgroundColor!.resolve({}),
      isNot(standard.style!.backgroundColor!.resolve({})),
    );
    expect(tester.takeException(), isNull);
  });
}
