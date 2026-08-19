CREATE TABLE senso_rake_schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TIMESTAMP NOT NULL,
    UNIQUE (version)
);
CREATE TABLE data_landing (
    ulid VARCHAR,
    ts TIMESTAMP,
    raw_json JSON
);
CREATE TABLE temperatures (
    ulid VARCHAR PRIMARY KEY,
    ts TIMESTAMP,
    model VARCHAR,
    id VARCHAR,
    channel VARCHAR,
    temperature_C FLOAT,
    battery_ok BIGINT,
    rssi FLOAT
);
CREATE TABLE humidities (
    ulid VARCHAR PRIMARY KEY,
    ts TIMESTAMP,
    model VARCHAR,
    id VARCHAR,
    channel VARCHAR,
    humidity FLOAT,
    battery_ok BIGINT,
    rssi FLOAT
);
CREATE TABLE pressures (
    ulid VARCHAR PRIMARY KEY,
    ts TIMESTAMP,
    model VARCHAR,
    id VARCHAR,
    channel VARCHAR,
    pressure_kPa FLOAT,
    battery_ok BIGINT,
    rssi FLOAT
);
CREATE SEQUENCE mapping_id_seq START 1;
CREATE TABLE mappings (
    mapping_id BIGINT PRIMARY KEY DEFAULT nextval('mapping_id_seq'),
    model VARCHAR,
    id VARCHAR,
    description VARCHAR,
    validity_start TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE VIEW temperatures_view AS
WITH desc_ranges AS (
    SELECT
        description,
        validity_start,
        LEAD(validity_start) OVER (
            PARTITION BY description
            ORDER BY validity_start
        ) AS validity_end
    FROM mappings
    WHERE NOT deleted
)
SELECT
    t.*,
    m.description
FROM temperatures t
JOIN mappings m
    ON t.model = m.model
   AND t.id = m.id
JOIN desc_ranges d
    ON m.description = d.description
   AND m.validity_start = d.validity_start
WHERE t.ts >= d.validity_start
  AND t.ts < COALESCE(d.validity_end, TIMESTAMP '9999-12-31');

CREATE VIEW pressures_view AS
WITH desc_ranges AS (
    SELECT
        description,
        validity_start,
        LEAD(validity_start) OVER (
            PARTITION BY description
            ORDER BY validity_start
        ) AS validity_end
    FROM mappings
    WHERE NOT deleted
)
SELECT
    p.*,
    m.description
FROM pressures p
JOIN mappings m
    ON p.model = m.model
   AND p.id = m.id
JOIN desc_ranges d
    ON m.description = d.description
   AND m.validity_start = d.validity_start
WHERE p.ts >= d.validity_start
  AND p.ts < COALESCE(d.validity_end, TIMESTAMP '9999-12-31');

CREATE VIEW humidities_view AS
WITH desc_ranges AS (
    SELECT
        description,
        validity_start,
        LEAD(validity_start) OVER (
            PARTITION BY description
            ORDER BY validity_start
        ) AS validity_end
    FROM mappings
    WHERE NOT deleted
)
SELECT
    h.*,
    m.description
FROM humidities h
JOIN mappings m
    ON h.model = m.model
   AND h.id = m.id
JOIN desc_ranges d
    ON m.description = d.description
   AND m.validity_start = d.validity_start
WHERE h.ts >= d.validity_start
  AND h.ts < COALESCE(d.validity_end, TIMESTAMP '9999-12-31');

CREATE VIEW latest_temperatures AS
WITH CTE AS (
    SELECT MAX(ULID) AS ULID
    FROM TEMPERATURES_VIEW
    GROUP BY DESCRIPTION
)
SELECT
    CTE.ULID,
    MODEL,
    ID,
    TEMPERATURE_C,
    TS,
    BATTERY_OK,
    RSSI,
    DESCRIPTION
FROM TEMPERATURES_VIEW
INNER JOIN CTE ON CTE.ULID = TEMPERATURES_VIEW.ULID;

CREATE VIEW latest_pressures AS
WITH CTE AS (
    SELECT MAX(ULID) AS ULID
    FROM PRESSURES_VIEW
    GROUP BY DESCRIPTION
)
SELECT
    CTE.ULID,
    MODEL,
    ID,
    PRESSURE_kPa,
    TS,
    BATTERY_OK,
    RSSI,
    DESCRIPTION
FROM PRESSURES_VIEW
INNER JOIN CTE ON CTE.ULID = PRESSURES_VIEW.ULID;

CREATE VIEW latest_humidities AS
WITH CTE AS (
    SELECT MAX(ULID) AS ULID
    FROM HUMIDITIES_VIEW
    GROUP BY DESCRIPTION
)
SELECT
    CTE.ULID,
    MODEL,
    ID,
    HUMIDITY,
    TS,
    BATTERY_OK,
    RSSI,
    DESCRIPTION
FROM HUMIDITIES_VIEW
INNER JOIN CTE ON CTE.ULID = HUMIDITIES_VIEW.ULID;

CREATE VIEW all_sensors AS
WITH CTE as (
SELECT 
    raw_json->>'model' AS model,
    raw_json->>'id' AS id,
    max(ulid) AS latest_ulid,
    max(ts) AS last_seen
FROM data_landing 
GROUP BY 
    raw_json->>'model',
    raw_json->>'id' 
ORDER BY max(ts) DESC
)
SELECT 
    mappings.mapping_id,
    cte.model,
    cte.id,
    cte.last_seen,
    cte.latest_ulid,
    mappings.description,
    mappings.validity_start,
    mappings.deleted
FROM cte 
LEFT JOIN mappings ON mappings.model = cte.model AND mappings.id = cte.id
ORDER BY mappings.mapping_id;
