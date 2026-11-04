import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/providers/notification_provider.dart';
import 'package:yayma/src/features/library/providers/library_provider.dart';
import 'package:yayma/src/rust/api/models.dart';

enum CreatePlaylistTrackStatus { success, createFailed, notFound, addFailed }

/// Separates creation from addition so the caller can report partial success.
Future<({CreatePlaylistTrackStatus status, int? kind})>
createPlaylistWithTrack({
  required String title,
  required String trackId,
  required String? albumId,
  required List<SimplePlaylistDto> Function() playlists,
  required Future<bool> Function(String title) create,
  required Future<bool> Function(int kind, String trackId, String? albumId) add,
}) async {
  final existingKinds = playlists().map((p) => p.kind).toSet();
  if (!await create(title)) {
    return (status: CreatePlaylistTrackStatus.createFailed, kind: null);
  }
  final created = playlists()
      .where((playlist) => !existingKinds.contains(playlist.kind))
      .toList();
  final matching = created
      .where((playlist) => playlist.title == title)
      .toList();
  final candidates = matching.isNotEmpty ? matching : created;
  if (candidates.length != 1) {
    return (status: CreatePlaylistTrackStatus.notFound, kind: null);
  }
  final kind = candidates.single.kind;
  if (!await add(kind, trackId, albumId)) {
    return (status: CreatePlaylistTrackStatus.addFailed, kind: kind);
  }
  return (status: CreatePlaylistTrackStatus.success, kind: kind);
}

Future<int?> promptCreatePlaylistWithTrack(
  BuildContext context, {
  required String trackId,
  required String? albumId,
}) async {
  final controller = TextEditingController();
  final title = await showDialog<String>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      title: const Text('Новый плейлист'),
      content: TextField(
        controller: controller,
        maxLength: 64,
        decoration: const InputDecoration(hintText: 'Название'),
        onSubmitted: (value) => Navigator.of(dialogContext).pop(value),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(),
          child: const Text('Отмена'),
        ),
        FilledButton(
          onPressed: () => Navigator.of(dialogContext).pop(controller.text),
          child: const Text('Создать'),
        ),
      ],
    ),
  ).whenComplete(controller.dispose);
  final trimmed = title?.trim();
  if (trimmed == null || trimmed.isEmpty || !context.mounted) return null;

  final result = await createPlaylistWithTrack(
    title: trimmed,
    trackId: trackId,
    albumId: albumId,
    playlists: () => playlistsSignal.value,
    create: (name) => createPlaylistAction(name, isPublic: false),
    add: addTrackToPlaylistAction,
  );
  switch (result.status) {
    case CreatePlaylistTrackStatus.success:
      showAppSuccess("Создан плейлист '$trimmed', трек добавлен");
      return result.kind;
    case CreatePlaylistTrackStatus.createFailed:
      showAppError('Ошибка при создании плейлиста');
    case CreatePlaylistTrackStatus.notFound:
      showAppError(
        'Плейлист создан, но его не удалось найти для добавления трека',
      );
    case CreatePlaylistTrackStatus.addFailed:
      showAppError('Плейлист создан, но трек не удалось добавить');
  }
  return null;
}
