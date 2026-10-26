import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/theme/app_tokens.dart';
import 'package:yayma/src/features/core/views/widgets/responsive.dart';

class AiContentReductionSetting extends StatefulWidget {
  final Future<bool> Function() read;
  final Future<bool> Function({required bool enabled}) write;
  final VoidCallback onSaveError;
  const AiContentReductionSetting({
    required this.read,
    required this.write,
    required this.onSaveError,
    super.key,
  });
  @override
  State<AiContentReductionSetting> createState() =>
      _AiContentReductionSettingState();
}

class _AiContentReductionSettingState extends State<AiContentReductionSetting> {
  bool? _value;
  bool _busy = true;
  bool _error = false;
  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = false;
    });
    try {
      final value = await widget.read();
      if (!mounted) return;
      setState(() {
        _value = value;
        _busy = false;
      });
    } on Object catch (_) {
      if (!mounted) return;
      setState(() {
        _value = null;
        _busy = false;
        _error = true;
      });
    }
  }

  Future<void> _save(bool enabled) async {
    if (_busy || _value == null) return;
    setState(() => _busy = true);
    try {
      final confirmed = await widget.write(enabled: enabled);
      if (confirmed != enabled) throw StateError('Setting not confirmed');
      if (!mounted) return;
      setState(() {
        _value = confirmed;
        _busy = false;
      });
    } on Object catch (_) {
      if (!mounted) return;
      setState(() {
        _value = null;
        _busy = false;
        _error = true;
      });
      widget.onSaveError();
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final narrow = context.isNarrow;
    return InkWell(
      borderRadius: BorderRadius.circular(AppRadius.sm),
      onTap: _busy
          ? null
          : _error
          ? _load
          : () => _save(!_value!),
      child: Padding(
        padding: EdgeInsets.symmetric(
          horizontal: narrow ? 8 : 12,
          vertical: narrow ? 8 : 10,
        ),
        child: Row(
          children: [
            Container(
              padding: EdgeInsets.all(narrow ? 8 : 10),
              decoration: BoxDecoration(
                color: colors.primary.withValues(alpha: 0.1),
                borderRadius: BorderRadius.circular(AppRadius.sm),
              ),
              child: Icon(
                Icons.auto_awesome_rounded,
                color: colors.primary,
                size: narrow ? 20 : 22,
              ),
            ),
            SizedBox(width: narrow ? 12 : 16),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Меньше ИИ',
                    style: TextStyle(
                      color: colors.onSurface,
                      fontSize: narrow ? 15 : 16,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const SizedBox(height: 3),
                  Text(
                    _error
                        ? 'Не удалось загрузить настройку. Нажмите, чтобы повторить.'
                        : 'Реже предлагать треки, созданные ИИ. Настройка общая для аккаунта.',
                    style: TextStyle(
                      color: colors.onSurfaceVariant,
                      fontSize: narrow ? 12 : 14,
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(width: 12),
            if (_busy)
              const SizedBox(
                width: 24,
                height: 24,
                child: CircularProgressIndicator(strokeWidth: 2),
              )
            else if (_error)
              IconButton(
                onPressed: _load,
                tooltip: 'Повторить',
                icon: const Icon(Icons.refresh_rounded),
              )
            else
              Switch(value: _value!, onChanged: _save),
          ],
        ),
      ),
    );
  }
}
