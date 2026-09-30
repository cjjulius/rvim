-- PostgreSQL sample
CREATE TABLE IF NOT EXISTS events (
    id          BIGSERIAL PRIMARY KEY,
    payload     JSONB NOT NULL,
    tags        TEXT[] DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO events (payload, tags)
VALUES ('{"kind": "signup"}'::jsonb, ARRAY['web', 'beta'])
RETURNING id;

SELECT
    e.id,
    e.payload ->> 'kind'         AS kind,
    date_trunc('day', e.created_at) AS day,
    array_agg(t)                 AS all_tags
FROM events AS e
CROSS JOIN LATERAL unnest(e.tags) AS t
WHERE e.created_at > now() - INTERVAL '7 days'
GROUP BY e.id, kind, day
ORDER BY day DESC
LIMIT 100;
