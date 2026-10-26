import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/app/session.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';
import 'package:yayma/src/rust/api/artist_stats.dart' as rust;
import 'package:yayma/src/rust/api/models.dart';

/// Independent lookups keep external statistics from blocking the artist page.
class ArtistAudience extends StatefulWidget {
  final String artistId;
  final int? yandexListeners;
  const ArtistAudience({
    required this.artistId,
    this.yandexListeners,
    super.key,
  });
  @override
  State<ArtistAudience> createState() => _ArtistAudienceState();
}

class _ArtistAudienceState extends State<ArtistAudience> {
  late final Future<SpotifyArtistStatsDto?> _spotify;
  @override
  void initState() {
    super.initState();
    final ctx = appContextSignal.value;
    _spotify = ctx == null
        ? Future.value()
        : rust
              .getArtistSpotifyStats(ctx: ctx, artistId: widget.artistId)
              .then<SpotifyArtistStatsDto?>((value) => value)
              .timeout(const Duration(seconds: 30), onTimeout: () => null);
  }

  Widget _badge(
    BuildContext context,
    String service,
    int? count, {
    bool loading = false,
  }) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        ClipOval(
          child: Image.asset(
            service == 'Spotify'
                ? 'assets/icons/spotify.png'
                : 'assets/icons/yandex_music.png',
            width: 24,
            height: 24,
            fit: BoxFit.cover,
            semanticLabel: service,
          ),
        ),
        const SizedBox(width: 8),
        Text(
          loading
              ? '…'
              : count == null
              ? '—'
              : '${formatCompactAudience(count)} в месяц',
          style: TextStyle(
            fontSize: 18,
            color: Theme.of(context).colorScheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: 24,
    runSpacing: 10,
    children: [
      _badge(context, 'Яндекс Музыка', widget.yandexListeners),
      FutureBuilder<SpotifyArtistStatsDto?>(
        future: _spotify,
        builder: (context, snapshot) => _badge(
          context,
          'Spotify',
          snapshot.data?.monthlyListeners?.toInt(),
          loading: snapshot.connectionState != ConnectionState.done,
        ),
      ),
    ],
  );
}
