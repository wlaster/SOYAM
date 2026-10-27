import 'package:flutter_test/flutter_test.dart';
import 'package:m3e_core/m3e_core.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';

Widget host() => const MaterialApp(
  home: Scaffold(body: Center(child: CommonVolumeSlider(width: 240))),
);

void main() {
  testWidgets('volume label follows the thumb and hides on idle and release', (
    tester,
  ) async {
    await tester.pumpWidget(host());
    await tester.pumpAndSettle();
    final slider = find.byType(M3ESlider);
    expect(tester.widget<M3ESlider>(slider).label, isNull);
    final center = tester.getCenter(slider);
    final gesture = await tester.startGesture(center);
    await tester.pump(const Duration(milliseconds: 150));
    await gesture.moveBy(const Offset(24, 0));
    await tester.pump(const Duration(milliseconds: 150));
    final firstLabel = tester.widget<M3ESlider>(slider).label!;
    expect(find.text(firstLabel), findsOneWidget);
    expect(firstLabel, matches(RegExp(r'^\d+$')));
    final firstX = tester.getCenter(find.text(firstLabel)).dx;
    await gesture.moveBy(const Offset(24, 0));
    await tester.pump(const Duration(milliseconds: 150));
    final secondLabel = tester.widget<M3ESlider>(slider).label!;
    expect(tester.getCenter(find.text(secondLabel)).dx, greaterThan(firstX));
    await tester.pump(const Duration(milliseconds: 700));
    expect(tester.widget<M3ESlider>(slider).label, isNull);
    await gesture.moveBy(const Offset(-12, 0));
    await tester.pump(const Duration(milliseconds: 150));
    expect(tester.widget<M3ESlider>(slider).label, isNotNull);
    await gesture.up();
    await tester.pump();
    expect(tester.widget<M3ESlider>(slider).label, isNull);
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets('disposing a volume slider cancels its idle timer', (
    tester,
  ) async {
    await tester.pumpWidget(host());
    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(M3ESlider)),
    );
    await tester.pump(const Duration(milliseconds: 150));
    await tester.pumpWidget(const SizedBox());
    await gesture.up();
    await tester.pump(const Duration(seconds: 1));
    expect(tester.takeException(), isNull);
  });
}
