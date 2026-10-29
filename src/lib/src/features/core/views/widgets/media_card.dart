import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/app_cover.dart';
import 'package:yayma/src/features/core/views/widgets/track_elements.dart';
import 'package:yayma/src/rust/api/models.dart';

class CommonMediaCard extends StatefulWidget {
  final String title;
  final List<TrackArtistDto>? artists;
  final String? subtitle;
  final String? coverUrl;
  final VoidCallback onTap;
  final double size;
  final double coverScale;
  final bool isCircle;
  final int? titleMaxLines;
  final EdgeInsetsGeometry contentPadding;

  const CommonMediaCard({
    required this.title,
    required this.onTap,
    super.key,
    this.artists,
    this.subtitle,
    this.coverUrl,
    this.size = 160,
    this.coverScale = 1,
    this.isCircle = false,
    this.titleMaxLines = 1,
    this.contentPadding = const EdgeInsets.all(8),
  });

  @override
  State<CommonMediaCard> createState() => _CommonMediaCardState();
}

class _CommonMediaCardState extends State<CommonMediaCard> {
  final ValueNotifier<bool> _isHovered = ValueNotifier(false);
  final ValueNotifier<bool> _isPressed = ValueNotifier(false);

  @override
  void dispose() {
    _isHovered.dispose();
    _isPressed.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => _isHovered.value = true,
      onExit: (_) => _isHovered.value = false,
      child: InkWell(
        onTap: widget.onTap,
        onHighlightChanged: (value) => _isPressed.value = value,
        borderRadius: BorderRadius.circular(
          widget.isCircle ? widget.size / 2 : 20,
        ),
        splashColor: Theme.of(
          context,
        ).colorScheme.primary.withValues(alpha: 0.06),
        child: Container(
          width: widget.size,
          padding: widget.contentPadding,
          child: ValueListenableBuilder<bool>(
            valueListenable: _isHovered,
            builder: (context, hovered, _) {
              return ValueListenableBuilder<bool>(
                valueListenable: _isPressed,
                builder: (context, pressed, _) {
                  return AnimatedScale(
                    scale: pressed ? 0.95 : (hovered ? 1.02 : 1.0),
                    duration: Duration(milliseconds: pressed ? 90 : 220),
                    curve: pressed ? Curves.easeOutCubic : Curves.easeOutBack,
                    child: Column(
                      crossAxisAlignment: widget.isCircle
                          ? CrossAxisAlignment.center
                          : CrossAxisAlignment.start,
                      children: [
                        AnimatedContainer(
                          duration: const Duration(milliseconds: 200),
                          curve: Curves.easeOut,
                          decoration: BoxDecoration(
                            borderRadius: BorderRadius.circular(
                              widget.isCircle ? widget.size / 2 : 16,
                            ),
                            boxShadow: [
                              BoxShadow(
                                color: Colors.black.withValues(alpha: 0.4),
                                blurRadius: 12,
                                offset: const Offset(0, 6),
                              ),
                            ],
                          ),
                          child: AnimatedScale(
                            duration: const Duration(milliseconds: 200),
                            curve: Curves.easeOutBack,
                            scale: hovered ? 1.05 : 1.0,
                            child: Transform.scale(
                              scale: widget.coverScale,
                              alignment: Alignment.centerLeft,
                              child: AppCover(
                                coverUrl: widget.coverUrl,
                                size: widget.size - 16,
                                radius: 16,
                                circle: widget.isCircle,
                              ),
                            ),
                          ),
                        ),
                        const SizedBox(height: 12),
                        Text(
                          widget.title,
                          style: TextStyle(
                            color: Theme.of(context).colorScheme.onSurface,
                            fontWeight: FontWeight.bold,
                            fontSize: 15,
                            decoration: hovered
                                ? TextDecoration.underline
                                : null,
                          ),
                          maxLines: widget.titleMaxLines,
                          overflow: widget.titleMaxLines == null
                              ? TextOverflow.visible
                              : TextOverflow.ellipsis,
                          textAlign: widget.isCircle
                              ? TextAlign.center
                              : TextAlign.start,
                        ),
                        if (widget.artists != null) ...[
                          const SizedBox(height: 4),
                          ArtistNamesWidget(
                            artists: widget.artists!,
                            fontSize: 13,
                            color: Theme.of(
                              context,
                            ).colorScheme.onSurface.withValues(alpha: 0.55),
                          ),
                        ] else if (widget.subtitle != null) ...[
                          const SizedBox(height: 4),
                          Text(
                            widget.subtitle!,
                            style: TextStyle(
                              color: Theme.of(
                                context,
                              ).colorScheme.onSurface.withValues(alpha: 0.55),
                              fontSize: 13,
                            ),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            textAlign: widget.isCircle
                                ? TextAlign.center
                                : TextAlign.start,
                          ),
                        ],
                      ],
                    ),
                  );
                },
              );
            },
          ),
        ),
      ),
    );
  }
}
