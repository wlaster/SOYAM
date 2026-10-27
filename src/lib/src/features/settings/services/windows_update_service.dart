import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:path_provider/path_provider.dart';
import 'package:yayma/src/features/settings/services/update_service.dart';

class WindowsUpdateAsset {
  const WindowsUpdateAsset({
    required this.name,
    required this.url,
    required this.size,
    this.digest,
    this.checksumUrl,
  });
  final String name;
  final Uri url;
  final int size;
  final String? digest;
  final Uri? checksumUrl;
}

class WindowsUpdateService {
  static final Map<String, Future<File>> _downloads = {};

  static bool isInstalledAt(String executable) {
    final directory = File(executable).parent;
    return directory.existsSync() &&
        directory.listSync().any(
          (entry) =>
              entry is File &&
              RegExp(
                r'^unins\d{3}\.exe$',
                caseSensitive: false,
              ).hasMatch(entry.uri.pathSegments.last),
        );
  }

  static bool get isInstalled =>
      Platform.isWindows && isInstalledAt(Platform.resolvedExecutable);

  static String _version(String version) {
    final value = version.replaceFirst(RegExp('^v'), '');
    if (!RegExp(r'^\d+\.\d+\.\d+$').hasMatch(value)) {
      throw const UpdateException('Некорректная версия обновления.');
    }
    return value;
  }

  static bool _trustedAsset(Uri uri) =>
      uri.scheme == 'https' &&
      uri.host == 'github.com' &&
      uri.path.startsWith('/wlaster/SOYAM/releases/download/');

  static WindowsUpdateAsset parseAsset(
    Map<String, dynamic> release,
    String version,
  ) {
    final normalized = _version(version);
    if (_version(release['tag_name'] as String) != normalized ||
        release['draft'] == true ||
        release['prerelease'] == true) {
      throw const UpdateException('Версия релиза не соответствует обновлению.');
    }
    final legacyName = 'soyam-windows-setup-$normalized.exe';
    final name =
        (release['assets'] as List).whereType<Map<String, dynamic>>().any(
          (asset) => asset['name'] == 'soyam-windows-setup.exe',
        )
        ? 'soyam-windows-setup.exe'
        : legacyName;
    final assets = (release['assets'] as List)
        .whereType<Map<String, dynamic>>()
        .toList();
    final matches = assets.where((asset) => asset['name'] == name).toList();
    if (matches.length != 1) {
      throw const UpdateException(
        'В релизе нет установщика SOYAM для Windows x64.',
      );
    }
    final raw = matches.single;
    final url = Uri.parse(raw['browser_download_url'] as String);
    final size = raw['size'] as int;
    if (!_trustedAsset(url) || size <= 0 || size > 512 * 1024 * 1024) {
      throw const UpdateException('Некорректный файл обновления.');
    }
    final digest = raw['digest'] as String?;
    final hash =
        digest != null && RegExp(r'^sha256:[a-fA-F0-9]{64}$').hasMatch(digest)
        ? digest.substring(7).toLowerCase()
        : null;
    Uri? checksumUrl;
    for (final asset in assets) {
      if (asset['name'] == '$name.sha256') {
        final uri = Uri.parse(asset['browser_download_url'] as String);
        if (_trustedAsset(uri)) checksumUrl = uri;
      }
    }
    if (hash == null && checksumUrl == null) {
      throw const UpdateException(
        'Для установщика отсутствует контрольная сумма SHA-256.',
      );
    }
    return WindowsUpdateAsset(
      name: name,
      url: url,
      size: size,
      digest: hash,
      checksumUrl: checksumUrl,
    );
  }

  static Future<HttpClientResponse> _get(HttpClient client, Uri url) async {
    final request = await client
        .getUrl(url)
        .timeout(const Duration(seconds: 20));
    request.headers.set('User-Agent', 'SOYAM-updater');
    request.headers.set('Accept', 'application/vnd.github+json');
    final response = await request.close().timeout(const Duration(seconds: 30));
    if (response.statusCode != HttpStatus.ok) {
      throw UpdateException(
        'Ошибка загрузки обновления: HTTP ${response.statusCode}.',
      );
    }
    return response;
  }

  static Future<WindowsUpdateAsset> resolveAsset(
    AppUpdateInfo info, {
    HttpClient? client,
  }) async {
    final http = client ?? HttpClient();
    try {
      _version(info.latestVersion);
      final page = Uri.parse(info.url);
      if (page.host != 'github.com' ||
          page.scheme != 'https' ||
          !page.path.startsWith('/wlaster/SOYAM/releases/tag/')) {
        throw const UpdateException('Неизвестный источник обновления.');
      }
      final tag = page.pathSegments.last;
      final response = await _get(
        http,
        Uri.https('api.github.com', '/repos/wlaster/SOYAM/releases/tags/$tag'),
      );
      final body = await response
          .timeout(const Duration(seconds: 30))
          .transform(utf8.decoder)
          .join();
      return parseAsset(
        jsonDecode(body) as Map<String, dynamic>,
        info.latestVersion,
      );
    } finally {
      http.close(force: true);
    }
  }

  static Future<File> download(
    WindowsUpdateAsset asset, {
    DownloadProgressCallback? onProgress,
    Directory? directory,
    HttpClient? client,
  }) async {
    final key = asset.url.toString();
    final existing = _downloads[key];
    if (existing != null) return await existing;
    final future = _download(
      asset,
      onProgress: onProgress,
      directory: directory,
      client: client,
    );
    unawaited(_downloads[key] = future);
    try {
      return await future;
    } finally {
      unawaited(_downloads.remove(key));
    }
  }

  static Future<File> _download(
    WindowsUpdateAsset asset, {
    DownloadProgressCallback? onProgress,
    Directory? directory,
    HttpClient? client,
  }) async {
    if (!_trustedAsset(asset.url) ||
        (asset.checksumUrl != null && !_trustedAsset(asset.checksumUrl!)) ||
        !RegExp(
          r'^soyam-windows-setup(?:-\d+\.\d+\.\d+)?\.exe$',
        ).hasMatch(asset.name)) {
      throw const UpdateException('Некорректный установщик обновления.');
    }
    final http = client ?? HttpClient();
    File? partial;
    try {
      var expected = asset.digest;
      if (expected == null && asset.checksumUrl != null) {
        final response = await _get(http, asset.checksumUrl!);
        final bytes = await response
            .timeout(const Duration(seconds: 30))
            .expand((chunk) => chunk)
            .take(4096)
            .toList();
        expected = RegExp(
          r'^[a-fA-F0-9]{64}(?=\s|$)',
        ).firstMatch(utf8.decode(bytes).trim())?.group(0)?.toLowerCase();
      }
      if (expected == null || !RegExp(r'^[a-f0-9]{64}$').hasMatch(expected)) {
        throw const UpdateException(
          'Не удалось проверить контрольную сумму установщика.',
        );
      }
      final root =
          directory ??
          Directory('${(await getTemporaryDirectory()).path}/soyam-updates');
      await root.create(recursive: true);
      final file = File('${root.path}/${asset.name}');
      if (file.existsSync() &&
          file.lengthSync() == asset.size &&
          (await sha256.bind(file.openRead()).first).toString() == expected) {
        onProgress?.call(1);
        return file;
      }
      partial = File('${file.path}.part');
      final response = await _get(http, asset.url);
      final sink = partial.openWrite();
      var received = 0;
      try {
        await for (final chunk in response.timeout(
          const Duration(seconds: 30),
        )) {
          received += chunk.length;
          if (received > asset.size) {
            throw const UpdateException('Неверный размер установщика.');
          }
          sink.add(chunk);
          onProgress?.call((received / asset.size).clamp(0.0, 1.0));
        }
        await sink.flush();
      } finally {
        await sink.close();
      }
      if (received != asset.size || received < 2) {
        throw const UpdateException(
          'Установщик загрузился не полностью. Повторите загрузку.',
        );
      }
      final hash = await sha256.bind(partial.openRead()).first;
      final header = await partial.open();
      late List<int> magic;
      try {
        magic = await header.read(2);
      } finally {
        await header.close();
      }
      if (hash.toString() != expected || magic[0] != 0x4d || magic[1] != 0x5a) {
        throw const UpdateException(
          'Проверка установщика не пройдена. Повторите загрузку.',
        );
      }
      await partial.rename(file.path);
      onProgress?.call(1);
      return file;
    } finally {
      http.close(force: true);
      if (partial != null && partial.existsSync()) await partial.delete();
    }
  }

  static Future<void> install(String path) async {
    if (!isInstalled) {
      throw const UpdateException(
        'Обновление через установщик доступно для установленной SOYAM.',
      );
    }
    await Process.start(path, ['/SP-'], mode: ProcessStartMode.detached);
    exit(0);
  }
}
