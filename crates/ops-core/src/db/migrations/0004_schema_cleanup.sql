-- Şema temizliği: hiçbir kod yolunun yazmadığı "gelecek özellik" kolonları
-- kaldırılır. Amaç, veri modelinin uygulamanın gerçekten yaptığını yansıtması.
--   tasks.confidence / inferred_status / last_evidence_at / user_confirmed_status:
--     otomatik statü çıkarımı uygulanmadı; kullanıcı beyanı tek doğruluk kaynağıdır.
--   routines.timezone / parameters / allowed_capabilities / approval_policy:
--     rutinler makine saat dilimini kullanır ve yalnızca bildirim üretir.
--   remote_messages.attachment_meta: ek dosyalar işlenmez.
--   settings.timezone: kullanılmıyor (UI kendi ofsetini gönderir).

ALTER TABLE tasks DROP COLUMN confidence;
ALTER TABLE tasks DROP COLUMN inferred_status;
ALTER TABLE tasks DROP COLUMN last_evidence_at;
ALTER TABLE tasks DROP COLUMN user_confirmed_status;

ALTER TABLE routines DROP COLUMN timezone;
ALTER TABLE routines DROP COLUMN parameters;
ALTER TABLE routines DROP COLUMN allowed_capabilities;
ALTER TABLE routines DROP COLUMN approval_policy;

ALTER TABLE remote_messages DROP COLUMN attachment_meta;

DELETE FROM settings WHERE key = 'timezone';
