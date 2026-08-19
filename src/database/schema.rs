pub const UPDATE_TABLES_SQL: &str = r#"
WITH cte AS (
  SELECT COALESCE(max(ulid), '00000000-0000-0000-0000-000000000000') AS ulid_max FROM temperatures
)
INSERT INTO temperatures (
  ulid, ts, model, id, channel, temperature_c, battery_ok, rssi
)
SELECT DISTINCT
  ulid,
  ts,
  raw_json->>'model',
  raw_json->>'id',
  (raw_json->>'channel'),
  (raw_json->>'temperature_C')::float,
  (raw_json->>'battery_ok')::int,
  (raw_json->>'rssi')::float
FROM data_landing
JOIN cte ON ulid > cte.ulid_max
WHERE json_exists(raw_json, '$.temperature_C')
ORDER BY ulid;

WITH cte AS (
  SELECT COALESCE(max(ulid), '00000000-0000-0000-0000-000000000000') AS ulid_max FROM humidities
)
INSERT INTO humidities (
  ulid, ts, model, id, channel, humidity, battery_ok, rssi
)
SELECT DISTINCT
  ulid,
  ts,
  raw_json->>'model',
  raw_json->>'id',
  (raw_json->>'channel'),
  (raw_json->>'humidity')::float,
  (raw_json->>'battery_ok')::int,
  (raw_json->>'rssi')::float
FROM data_landing
JOIN cte ON ulid > cte.ulid_max
WHERE json_exists(raw_json, '$.humidity')
ORDER BY ulid;

WITH cte AS (
  SELECT COALESCE(max(ulid), '00000000-0000-0000-0000-000000000000') AS ulid_max FROM pressures
)
INSERT INTO pressures (
  ulid, ts, model, id, channel, pressure_kPa, battery_ok, rssi
)
SELECT DISTINCT
  ulid,
  ts,
  raw_json->>'model',
  raw_json->>'id',
  (raw_json->>'channel'),
  (raw_json->>'pressure_kPa')::float,
  (raw_json->>'battery_ok')::int,
  (raw_json->>'rssi')::float
FROM data_landing
JOIN cte ON ulid > cte.ulid_max
WHERE json_exists(raw_json, '$.pressure_kPa')
ORDER BY ulid;
"#;
