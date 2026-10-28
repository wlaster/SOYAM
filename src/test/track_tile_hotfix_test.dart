import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/track_tile.dart';

void main() {
  testWidgets('mouse clicks on title, cover and row start playback once', (
    tester,
  ) async {
    var plays = 0;
    var navigations = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(
            child: SizedBox(
              width: 700,
              child: CommonTrackTile(
                trackId: '11:22',
                title: 'Test track',
                artists: const [],
                durationMs: 60000,
                leading: const SizedBox(
                  key: ValueKey('cover'),
                  width: 64,
                  height: 64,
                  child: ColoredBox(color: Colors.grey),
                ),
                onTap: () => plays++,
                onTitleTap: () => navigations++,
                isExplicit: true,
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    for (final target in [
      find.text('Test track'),
      find.byKey(const ValueKey('cover')),
    ]) {
      await tester.tap(target, kind: PointerDeviceKind.mouse);
      await tester.pumpAndSettle();
    }
    final row = tester.getRect(find.byType(CommonTrackTile));
    await tester.tapAt(
      Offset(row.left + 200, row.bottom - 5),
      kind: PointerDeviceKind.mouse,
    );
    await tester.pumpAndSettle();
    expect(plays, 3);
    expect(navigations, 0);
    expect(find.text('E'), findsOneWidget);
    expect(find.byTooltip('Взрослый контент'), findsOneWidget);
  });
  testWidgets('clean tracks have no explicit badge', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: CommonTrackTile(trackId: '1', title: 'Clean', artists: []),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('E'), findsNothing);
  });
}
