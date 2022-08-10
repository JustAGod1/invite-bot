CREATE TABLE users (
    id SERIAL PRIMARY KEY ,
    full_name VARCHAR NOT NULL,
    telegram_id VARCHAR,
    phone VARCHAR
    )