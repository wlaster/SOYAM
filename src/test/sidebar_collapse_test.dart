import 'package:flutter/gestures.dart';
import 'package:yayma/src/app/session.dart';
import 'package:yayma/src/rust/api/models.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/providers/navigation_provider.dart';
import 'package:yayma/src/features/core/views/layout/navigation_bar.dart';
import 'package:yayma/src/features/core/views/widgets/sidebar_collapse.dart';

void main() {
  testWidgets('profile hover highlights a circle and clears on exit', (
    tester,
  ) async {
    accountSignal.value = const UserAccountDto(
      uid: 1,
      login: 'test',
      hasPlus: false,
    );
    addTearDown(() => accountSignal.value = null);
    await tester.pumpWidget(
      const MaterialApp(home: Scaffold(body: FloatingNavBar())),
    );
    final target = find.byKey(const ValueKey('profile-hover'));
    BoxDecoration decoration() =>
        tester.widget<AnimatedContainer>(target).decoration! as BoxDecoration;
    expect(decoration().shape, BoxShape.circle);
    expect(decoration().color!.a, 0);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(700, 100));
    await mouse.moveTo(tester.getCenter(target));
    await tester.pumpAndSettle();
    expect(decoration().color!.a, closeTo(0.18, 0.01));
    expect(decoration().shape, BoxShape.circle);
    await mouse.moveTo(const Offset(700, 100));
    await tester.pumpAndSettle();
    expect(decoration().color!.a, 0);
    await mouse.removePointer();
  });

  testWidgets(
    'rail folds to selected button before sliding left and can reverse',
    (tester) async {
      final visible = ValueNotifier(true);
      addTearDown(visible.dispose);
      final selected = GlobalKey();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: ValueListenableBuilder<bool>(
                valueListenable: visible,
                builder: (context, value, _) => SidebarCollapse(
                  visible: value,
                  selectedKey: selected,
                  color: Colors.black54,
                  selectedChild: const SizedBox(
                    key: ValueKey('collapsed-button'),
                    width: 48,
                    height: 48,
                  ),
                  child: SizedBox(
                    width: 64,
                    height: 320,
                    child: Column(
                      children: [
                        const SizedBox(height: 120),
                        SizedBox(key: selected, width: 48, height: 48),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      final selectedY = tester.getCenter(find.byKey(selected)).dy;
      visible.value = false;
      await tester.pump();
      var previousHeight = 320.0;
      for (var frame = 0; frame < 8; frame++) {
        await tester.pump(const Duration(milliseconds: 50));
        final background = tester.getRect(
          find.byKey(const ValueKey('sidebar-background')),
        );
        final button = tester.getRect(
          find.byKey(const ValueKey('collapsed-button')),
        );
        expect(background.height, lessThanOrEqualTo(previousHeight));
        expect(background.height, greaterThanOrEqualTo(72));
        expect(background.top, lessThanOrEqualTo(button.top - 4));
        expect(background.bottom, greaterThanOrEqualTo(button.bottom + 4));
        previousHeight = background.height;
      }
      expect(
        tester
            .widget<Transform>(
              find.byKey(const ValueKey('sidebar-translation')),
            )
            .transform
            .storage[12],
        0,
      );
      expect(
        tester.getCenter(find.byKey(const ValueKey('collapsed-button'))).dy,
        selectedY,
      );
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<Transform>(
              find.byKey(const ValueKey('sidebar-translation')),
            )
            .transform
            .storage[12],
        -140,
      );
      visible.value = true;
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 100));
      visible.value = false;
      await tester.pumpAndSettle();
      visible.value = true;
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<Transform>(
              find.byKey(const ValueKey('sidebar-translation')),
            )
            .transform
            .storage[12],
        0,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('auto-hide applies to search, library and nested detail pages', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1200, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    autoHideNavbarSignal.value = true;
    addTearDown(() {
      autoHideNavbarSignal.value = false;
      setSection(AppSection.home);
    });
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: Align(alignment: Alignment.centerLeft, child: FloatingNavBar()),
        ),
      ),
    );
    for (final section in [
      AppSection.search,
      AppSection.liked,
      AppSection.album,
      AppSection.artist,
      AppSection.account,
    ]) {
      navigateTo(
        section,
        section == AppSection.album || section == AppSection.artist
            ? '1'
            : null,
      );
      await tester.pumpAndSettle();
      expect(
        tester.widget<SidebarCollapse>(find.byType(SidebarCollapse)).visible,
        isFalse,
      );
    }
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(600, 100));
    await mouse.moveTo(
      tester.getCenter(find.byKey(const ValueKey('sidebar-edge-trigger'))),
    );
    await tester.pumpAndSettle();
    expect(
      tester.widget<SidebarCollapse>(find.byType(SidebarCollapse)).visible,
      isTrue,
    );
    final rail = tester.getRect(find.byType(SidebarCollapse));
    final trigger = tester.getRect(
      find.byKey(const ValueKey('sidebar-edge-trigger')),
    );
    expect(trigger.left, lessThanOrEqualTo(rail.left - 8));
    expect(trigger.right, greaterThanOrEqualTo(rail.right + 8));
    expect(trigger.top, lessThanOrEqualTo(rail.top - 8));
    expect(trigger.bottom, greaterThanOrEqualTo(rail.bottom + 8));
    await tester.tap(find.byIcon(Icons.search_rounded));
    await tester.pumpAndSettle();
    expect(currentNavStateSignal.value.section, AppSection.search);
    await mouse.moveTo(Offset(rail.left + 32, rail.center.dy));
    await tester.pumpAndSettle();
    expect(
      tester.widget<SidebarCollapse>(find.byType(SidebarCollapse)).visible,
      isTrue,
    );
    await mouse.moveTo(const Offset(600, 100));
    await tester.pumpAndSettle();
    expect(
      tester.widget<SidebarCollapse>(find.byType(SidebarCollapse)).visible,
      isFalse,
    );
    await mouse.removePointer();
    autoHideNavbarSignal.value = false;
    await tester.pumpAndSettle();
    expect(
      tester.widget<SidebarCollapse>(find.byType(SidebarCollapse)).visible,
      isTrue,
    );
    expect(tester.takeException(), isNull);
  });
}
