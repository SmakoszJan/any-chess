DROP INDEX IF EXISTS ip_idx;
DROP TABLE IF EXISTS room CASCADE;
DROP TABLE IF EXISTS event;

CREATE TABLE room (
    room_id serial PRIMARY KEY,
    white_taken boolean NOT NULL,
    name text,
    open boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    created_by bigint
);

CREATE INDEX ip_idx ON room(created_by);

CREATE TABLE event (
    room_id integer NOT NULL REFERENCES room ON DELETE CASCADE,
    time timestamptz NOT NULL DEFAULT now(),
    payload jsonb NOT NULL,
    PRIMARY KEY (room_id, time)
);
