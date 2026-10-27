import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:m3e_core/m3e_core.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/audio_settings.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';
import 'package:yayma/src/rust/api/models.dart';

void main() {
  testWidgets('standard curves are locked; a new custom profile is editable', (
    tester,
  ) async {
    final directory = Directory.systemTemp.createTempSync('soyam-eq-menu-');
    addTearDown(() => directory.deleteSync(recursive: true));
    tester.view.physicalSize = const Size(1200, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      const MethodChannel('plugins.flutter.io/path_provider'),
      (_) async => directory.path,
    );
    equalizerSignal.value = EqualizerDto(
      enabled: true,
      bands: List.generate(
        15,
        (index) => BandDto(index: index, frequency: 100, gainDb: 0),
      ),
    );
    addTearDown(() => equalizerSignal.value = null);
    await tester.pumpWidget(
      const MaterialApp(home: Scaffold(body: AudioSettingsDialog())),
    );
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 100)),
    );
    await tester.pumpAndSettle();
    final sliders = find.byType(M3ESlider);
    expect(sliders, findsNWidgets(15));
    expect(
      tester.widgetList<M3ESlider>(sliders).every((slider) => !slider.enabled),
      isTrue,
    );
    await tester.tap(find.byTooltip('Создать свой профиль (0/4)'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'Проверка');
    await tester.tap(find.text('Сохранить'));
    await tester.pumpAndSettle();
    for (var i = 0; i < 12; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 50)),
      );
      await tester.pumpAndSettle();
    }
    expect(
      tester.widgetList<M3ESlider>(sliders).every((slider) => slider.enabled),
      isTrue,
    );
    expect(find.text('Проверка'), findsOneWidget);
    expect(find.text('Изменения сохраняются в своём профиле'), findsNothing);
    expect(
      File('${directory.path}/equalizer_profiles.json').existsSync(),
      isTrue,
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 2));
  });
}
