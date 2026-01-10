pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS data_landing (
    ulid VARCHAR,
    ts TIMESTAMP,
    raw_json JSON
);
CREATE TABLE IF NOT EXISTS temperatures (
    ulid VARCHAR PRIMARY KEY,
    ts TIMESTAMP,
    model VARCHAR,
    id VARCHAR,
    channel BIGINT,
    temperature_C FLOAT,
    battery_ok BIGINT,
    rssi FLOAT
);
CREATE TABLE IF NOT EXISTS humidities (
    ulid VARCHAR PRIMARY KEY,
    ts TIMESTAMP,
    model VARCHAR,
    id VARCHAR,
    channel BIGINT,
    humidity FLOAT,
    battery_ok BIGINT,
    rssi FLOAT
);
CREATE TABLE IF NOT EXISTS pressures (
    ulid VARCHAR PRIMARY KEY,
    ts TIMESTAMP,
    model VARCHAR,
    id VARCHAR,
    channel BIGINT,
    pressure_kPa FLOAT,
    battery_ok BIGINT,
    rssi FLOAT
);
"#;

pub const UPDATE_TABLES_SQL: &str = r#"
WITH cte AS (
  SELECT max(ulid) AS ulid_max FROM temperatures
)
INSERT INTO temperatures (
  ulid, ts, model, id, channel, temperature_c, battery_ok, rssi
)
SELECT
  ulid,
  ts,
  raw_json->>'model',
  raw_json->>'id',
  (raw_json->>'channel')::int,
  (raw_json->>'temperature_C')::float,
  (raw_json->>'battery_ok')::int,
  (raw_json->>'rssi')::float
FROM data_landing
JOIN cte ON ulid > cte.ulid_max
WHERE json_exists(raw_json, '$.temperature_C');
---
WITH cte AS (
  SELECT max(ulid) AS ulid_max FROM humidities
)
INSERT INTO humidities (
  ulid, ts, model, id, channel, humidity, battery_ok, rssi
)
SELECT
  ulid,
  ts,
  raw_json->>'model',
  raw_json->>'id',
  (raw_json->>'channel')::int,
  (raw_json->>'humidity')::float,
  (raw_json->>'battery_ok')::int,
  (raw_json->>'rssi')::float
FROM data_landing
JOIN cte ON ulid > cte.ulid_max
WHERE json_exists(raw_json, '$.humidity');
---
WITH cte AS (
  SELECT max(ulid) AS ulid_max FROM pressures
)
INSERT INTO pressures (
  ulid, ts, model, id, channel, pressure_kPa, battery_ok, rssi
)
SELECT
  ulid,
  ts,
  raw_json->>'model',
  raw_json->>'id',
  (raw_json->>'channel')::int,
  (raw_json->>'pressure_kPa')::float,
  (raw_json->>'battery_ok')::int,
  (raw_json->>'rssi')::float
FROM data_landing
JOIN cte ON ulid > cte.ulid_max
WHERE json_exists(raw_json, '$.pressure_kPa');
"#;
