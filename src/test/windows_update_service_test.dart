import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:yayma/src/features/settings/services/update_service.dart';
import 'package:yayma/src/features/settings/services/windows_update_service.dart';

const name = 'soyam-windows-setup-1.2.0.exe';
const assetUrl =
    'https://github.com/wlaster/SOYAM/releases/download/v1.2.0/$name';
final List<int> payload = utf8.encode('MZ-SOYAM installer fixture');
String get digest => sha256.convert(payload).toString();

Map<String, dynamic> release({bool checksum = false}) => {
  'tag_name': 'v1.2.0',
  'draft': false,
  'prerelease': false,
  'assets': [
    {
      'name': name,
      'browser_download_url': assetUrl,
      'size': payload.length,
      if (!checksum) 'digest': 'sha256:$digest',
    },
    if (checksum)
      {'name': '$name.sha256', 'browser_download_url': '$assetUrl.sha256'},
    {'name': 'soyam-windows-x64-1.2.0-portable.zip'},
  ],
};

class Headers extends Fake implements HttpHeaders {
  @override
  void set(String name, Object value, {bool preserveHeaderCase = false}) {}
}

class Response extends Stream<List<int>> implements HttpClientResponse {
  Response(this.bytes, {this.statusCode = 200});
  final List<int> bytes;
  @override
  final int statusCode;
  @override
  int get contentLength => bytes.length;
  @override
  StreamSubscription<List<int>> listen(
    void Function(List<int>)? onData, {
    Function? onError,
    void Function()? onDone,
    bool? cancelOnError,
  }) =>
      Stream<List<int>>.fromIterable([
        bytes.take(2).toList(),
        bytes.skip(2).toList(),
      ]).listen(
        onData,
        onError: onError,
        onDone: onDone,
        cancelOnError: cancelOnError,
      );
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class Request extends Fake implements HttpClientRequest {
  Request(this.response);
  final Response response;
  @override
  HttpHeaders get headers => Headers();
  @override
  Future<HttpClientResponse> close() async => response;
}

class Client extends Fake implements HttpClient {
  Client(this.responses);
  final Map<String, Response> responses;
  @override
  Future<HttpClientRequest> getUrl(Uri uri) async =>
      Request(responses[uri.toString()] ?? Response([], statusCode: 404));
  @override
  void close({bool force = false}) {}
}

void main() {
  test(
    'downloads stable installer using GitHub digest without SHA files',
    () async {
      final data = release();
      final stableUrl = assetUrl.replaceAll(name, 'soyam-windows-setup.exe');
      final raw = (data['assets'] as List).first as Map<String, dynamic>;
      raw['name'] = 'soyam-windows-setup.exe';
      raw['browser_download_url'] = stableUrl;
      final asset = WindowsUpdateService.parseAsset(data, '1.2.0');
      expect(asset.name, 'soyam-windows-setup.exe');
      expect(asset.checksumUrl, isNull);
      final temp = Directory.systemTemp.createTempSync('soyam-stable-update-');
      try {
        final file = await WindowsUpdateService.download(
          asset,
          directory: temp,
          client: Client({stableUrl: Response(payload)}),
        );
        expect(await file.readAsBytes(), payload);
      } finally {
        temp.deleteSync(recursive: true);
      }
    },
  );

  late Directory directory;
  setUp(
    () => directory = Directory.systemTemp.createTempSync('soyam-update-test-'),
  );
  tearDown(() => directory.deleteSync(recursive: true));

  test('distinguishes installed application from portable', () {
    final executable = '${directory.path}/soyam.exe';
    File(executable).writeAsStringSync('MZ');
    expect(WindowsUpdateService.isInstalledAt(executable), isFalse);
    File('${directory.path}/unins000.exe').writeAsStringSync('MZ');
    expect(WindowsUpdateService.isInstalledAt(executable), isTrue);
  });

  test(
    'selects exact Windows installer and rejects another version and source',
    () {
      final asset = WindowsUpdateService.parseAsset(release(), '1.2.0');
      expect(asset.name, name);
      expect(asset.digest, digest);
      expect(
        () => WindowsUpdateService.parseAsset(release(), '1.3.0'),
        throwsA(isA<UpdateException>()),
      );
      final wrongSource = release();
      ((wrongSource['assets'] as List).first
              as Map<String, dynamic>)['browser_download_url'] =
          'https://example.com/$name';
      expect(
        () => WindowsUpdateService.parseAsset(wrongSource, '1.2.0'),
        throwsA(isA<UpdateException>()),
      );
    },
  );

  test(
    'resolves release metadata, downloads and verifies installer with progress',
    () async {
      final client = Client({
        'https://api.github.com/repos/wlaster/SOYAM/releases/tags/v1.2.0':
            Response(utf8.encode(jsonEncode(release()))),
        assetUrl: Response(payload),
      });
      final asset = await WindowsUpdateService.resolveAsset(
        const AppUpdateInfo(
          latestVersion: 'v1.2.0',
          changelog: '',
          url: 'https://github.com/wlaster/SOYAM/releases/tag/v1.2.0',
          hasUpdate: true,
        ),
        client: client,
      );
      final progress = <double?>[];
      final file = await WindowsUpdateService.download(
        asset,
        client: client,
        directory: directory,
        onProgress: progress.add,
      );
      expect(await file.readAsBytes(), payload);
      expect(progress.last, 1);
      expect(File('${file.path}.part').existsSync(), isFalse);
    },
  );

  test(
    'supports published checksum file when GitHub digest is absent',
    () async {
      final asset = WindowsUpdateService.parseAsset(
        release(checksum: true),
        '1.2.0',
      );
      final file = await WindowsUpdateService.download(
        asset,
        directory: directory,
        client: Client({
          '$assetUrl.sha256': Response(utf8.encode('$digest  $name\n')),
          assetUrl: Response(payload),
        }),
      );
      expect(file.lengthSync(), payload.length);
    },
  );

  test(
    'corrupt, truncated and failed downloads never leave an installable file',
    () async {
      final asset = WindowsUpdateService.parseAsset(release(), '1.2.0');
      for (final response in [
        Response(List.filled(payload.length, 1)),
        Response([77, 90]),
        Response([], statusCode: 500),
      ]) {
        await expectLater(
          WindowsUpdateService.download(
            asset,
            directory: directory,
            client: Client({assetUrl: response}),
          ),
          throwsA(isA<UpdateException>()),
        );
        expect(directory.listSync(), isEmpty);
      }
    },
  );
}
