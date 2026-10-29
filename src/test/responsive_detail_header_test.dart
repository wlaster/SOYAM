import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';

void main() {
  testWidgets('detail header adapts long titles and actions to window width', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPhysicalSize);
    for (final width in [320.0, 640.0, 850.0, 1100.0]) {
      tester.view.physicalSize = Size(width, 900);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: CommonDetailHeader(
                title:
                    'A very long album title to verify wrapping inside a small window',
                type: 'Album',
                actions: [
                  TextButton(onPressed: () {}, child: const Text('Play')),
                  TextButton(onPressed: () {}, child: const Text('Download')),
                ],
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull, reason: 'window width: $width');
    }
  });
}
