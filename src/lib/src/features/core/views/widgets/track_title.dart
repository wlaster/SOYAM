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
