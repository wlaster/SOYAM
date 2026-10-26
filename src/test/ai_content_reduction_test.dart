import 'dart:async';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/settings/views/ai_content_reduction_setting.dart';

Widget host({
  required Future<bool> Function() read,
  required Future<bool> Function({required bool enabled}) write,
  VoidCallback? onError,
}) => MaterialApp(
  home: Scaffold(
    body: AiContentReductionSetting(
      read: read,
      write: write,
      onSaveError: onError ?? () {},
    ),
  ),
);

void main() {
  testWidgets(
    'loads the existing server preference instead of defaulting to off',
    (tester) async {
      await tester.pumpWidget(
        host(
          read: () async => true,
          write: ({required bool enabled}) async => enabled,
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
    },
  );

  testWidgets('waits for server confirmation and prevents duplicate writes', (
    tester,
  ) async {
    final confirmation = Completer<bool>();
    var calls = 0;
    await tester.pumpWidget(
      host(
        read: () async => false,
        write: ({required bool enabled}) {
          calls++;
          expect(enabled, isTrue);
          return confirmation.future;
        },
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byType(Switch));
    await tester.pump();
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    expect(find.byType(Switch), findsNothing);
    await tester.tap(find.text('Меньше ИИ'));
    await tester.pump();
    expect(calls, 1);
    confirmation.complete(true);
    await tester.pumpAndSettle();
    expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
  });

  testWidgets('failed save shows retry and reloads actual account state', (
    tester,
  ) async {
    var server = false;
    var reads = 0;
    var errors = 0;
    await tester.pumpWidget(
      host(
        read: () async {
          reads++;
          return server;
        },
        write: ({required bool enabled}) async => throw StateError('offline'),
        onError: () => errors++,
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();
    expect(errors, 1);
    expect(find.byType(Switch), findsNothing);
    expect(find.byTooltip('Повторить'), findsOneWidget);
    server = true;
    await tester.tap(find.byTooltip('Повторить'));
    await tester.pumpAndSettle();
    expect(reads, 2);
    expect(tester.widget<Switch>(find.byType(Switch)).value, isTrue);
  });
}
