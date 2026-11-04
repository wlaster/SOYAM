import 'package:flutter_test/flutter_test.dart';
import 'package:yayma/src/features/library/views/create_playlist_with_track.dart';
import 'package:yayma/src/rust/api/models.dart';

SimplePlaylistDto playlist(int kind, String title) => SimplePlaylistDto(
  kind: kind,
  uid: 1,
  title: title,
  trackCount: 0,
  isPublic: false,
);

void main() {
  test(
    'creates a playlist and adds the track to the matching new playlist',
    () async {
      final items = [playlist(1, 'old')];
      final calls = <String>[];
      final result = await createPlaylistWithTrack(
        title: 'new',
        trackId: 'track',
        albumId: 'album',
        playlists: () => items,
        create: (name) async {
          items.addAll([playlist(2, 'unrelated'), playlist(3, name)]);
          return true;
        },
        add: (kind, track, album) async {
          calls.add('$kind:$track:$album');
          return true;
        },
      );
      expect(result.status, CreatePlaylistTrackStatus.success);
      expect(result.kind, 3);
      expect(calls, ['3:track:album']);
    },
  );

  test('reports a failed add without claiming success', () async {
    final items = <SimplePlaylistDto>[];
    final result = await createPlaylistWithTrack(
      title: 'new',
      trackId: 'track',
      albumId: null,
      playlists: () => items,
      create: (name) async {
        items.add(playlist(2, name));
        return true;
      },
      add: (_, _, _) async => false,
    );
    expect(result.status, CreatePlaylistTrackStatus.addFailed);
    expect(result.kind, 2);
  });

  test('does not add to an ambiguous or failed creation', () async {
    final items = <SimplePlaylistDto>[];
    var added = false;
    Future<bool> add(int kind, String track, String? album) async {
      added = true;
      return true;
    }

    final failed = await createPlaylistWithTrack(
      title: 'new',
      trackId: 'track',
      albumId: null,
      playlists: () => items,
      create: (_) async => false,
      add: add,
    );
    expect(failed.status, CreatePlaylistTrackStatus.createFailed);
    final ambiguous = await createPlaylistWithTrack(
      title: 'new',
      trackId: 'track',
      albumId: null,
      playlists: () => items,
      create: (_) async {
        items.addAll([playlist(1, 'new'), playlist(2, 'new')]);
        return true;
      },
      add: add,
    );
    expect(ambiguous.status, CreatePlaylistTrackStatus.notFound);
    expect(added, isFalse);
  });
}
