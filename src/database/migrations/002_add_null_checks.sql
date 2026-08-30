DROP VIEW all_sensors;
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
WHERE cte.model IS NOT NULL
  AND cte.id IS NOT NULL
  AND trim(cte.model) <> ''
  AND trim(cte.id) <> ''
ORDER BY mappings.mapping_id;

CREATE TABLE mappings_temp AS SELECT * FROM mappings;
DROP TABLE mappings;
CREATE TABLE mappings (
    mapping_id BIGINT PRIMARY KEY DEFAULT nextval('mapping_id_seq'),
    model VARCHAR,
    id VARCHAR,
    description VARCHAR,
    validity_start TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE(model, id, description, validity_start)
);
INSERT INTO mappings (SELECT * FROM mappings_temp);
DROP TABLE mappings_temp;