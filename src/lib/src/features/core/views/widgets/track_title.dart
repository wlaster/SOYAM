import 'package:material_ui/material_ui.dart';

/// Shared song title metrics from the desktop player. Weight stays local.
TextStyle trackTitleStyle(
  BuildContext context, {
  FontWeight weight = FontWeight.w800,
  TextDecoration? decoration,
}) => TextStyle(
  fontFamily: 'Google Sans',
  color: Theme.of(context).colorScheme.onSurface,
  fontSize: 17,
  fontWeight: weight,
  height: 1.15,
  letterSpacing: -0.4,
  decoration: decoration,
);

class ExplicitTrackBadge extends StatelessWidget {
  const ExplicitTrackBadge({super.key});
  @override
  Widget build(BuildContext context) {
    final color = Theme.of(context).colorScheme.onSurfaceVariant;
    return Tooltip(
      message: 'Взрослый контент',
      child: Container(
        margin: const EdgeInsets.only(left: 6),
        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
        decoration: BoxDecoration(
          color: color.withValues(alpha: 0.18),
          borderRadius: BorderRadius.circular(3),
        ),
        child: Text(
          'E',
          style: TextStyle(
            fontSize: 10,
            fontWeight: FontWeight.w700,
            color: color,
          ),
        ),
      ),
    );
  }
}
