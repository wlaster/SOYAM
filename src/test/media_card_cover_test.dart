import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/app_cover.dart';
import 'package:yayma/src/features/core/views/widgets/media_card.dart';

void main() {
  testWidgets('album card uses rounded square and soft cover shadow', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: CommonMediaCard(title: 'Album', onTap: () {}),
        ),
      ),
    );
    final cover = tester.widget<AppCover>(find.byType(AppCover));
    expect(cover.shape, isNull);
    expect(cover.radius, 16);
    final decorations = tester.widgetList<AnimatedContainer>(
      find.byType(AnimatedContainer),
    );
    expect(
      decorations.any(
        (widget) =>
            widget.decoration is BoxDecoration &&
            (widget.decoration! as BoxDecoration).boxShadow?.first.blurRadius ==
                12,
      ),
      isTrue,
    );
    expect(tester.takeException(), isNull);
  });
}
