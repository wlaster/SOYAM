import 'dart:io';
import 'package:flutter/foundation.dart';
import 'package:signals_flutter/signals_flutter.dart';

const double minVibeRenderScale = 0.25;
const double maxVibeRenderScale = 0.50;

final FlutterSignal<bool> vibeVisibleSignal = signal(true);
final FlutterSignal<double> vibeRenderScaleSignal = signal(0.50);
final FlutterSignal<bool> blurEffectsEnabledSignal = signal(true);

enum ProgressAppearance { standard, android, visualizer }

final FlutterSignal<ProgressAppearance> progressAppearanceSignal = signal(
  ProgressAppearance.standard,
);
File? _progressPreferences;
Future<void> _progressWrite = Future.value();
Future<void> loadProgressAppearance(String directory) async {
  _progressPreferences = File(
    '$directory${Platform.pathSeparator}progress-appearance.txt',
  );
  try {
    final name = await _progressPreferences!.readAsString();
    progressAppearanceSignal.value = ProgressAppearance.values.firstWhere(
      (v) => v.name == name.trim(),
      orElse: () => ProgressAppearance.standard,
    );
  } on FileSystemException catch (_) {}
}

Future<void> setProgressAppearance(ProgressAppearance value) {
  progressAppearanceSignal.value = value;
  return _progressWrite = _progressWrite.then((_) async {
    try {
      await _progressPreferences?.writeAsString(value.name, flush: true);
    } on FileSystemException catch (e) {
      debugPrint('Progress appearance save failed: $e');
    }
  });
}
