DROP TABLE IF EXISTS room CASCADE;
DROP TABLE IF EXISTS event;

CREATE TABLE room (
    room_id serial PRIMARY KEY,
    white_taken boolean NOT NULL,
    name text,
    open boolean NOT NULL DEFAULT true
);

CREATE TABLE event (
    room_id integer NOT NULL,
    time timestamptz NOT NULL DEFAULT now(),
    payload text NOT NULL,
    CONSTRAINT pk_event
        PRIMARY KEY (room_id, time),
    CONSTRAINT fk_room
        FOREIGN KEY (room_id)
        REFERENCES room(room_id)
        ON DELETE CASCADE
);
