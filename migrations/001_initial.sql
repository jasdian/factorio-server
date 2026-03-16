CREATE TABLE seasons (
    id          INTEGER PRIMARY KEY,
    status      TEXT NOT NULL CHECK(status IN ('pending','active','archived')),
    started_at  TEXT NOT NULL,
    ends_at     TEXT NOT NULL,
    map_seed    TEXT,
    save_path   TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE registrations (
    id              TEXT PRIMARY KEY,
    season_id       INTEGER NOT NULL REFERENCES seasons(id),
    factorio_name   TEXT NOT NULL,
    eth_address     TEXT NOT NULL,
    promo_code      TEXT,
    tx_hash         TEXT,
    status          TEXT NOT NULL CHECK(status IN ('awaiting_payment','confirmed','expired')),
    access_tier     TEXT NOT NULL CHECK(access_tier IN ('standard','instant_player'))
                    DEFAULT 'standard',
    amount_wei      TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    confirmed_at    TEXT
);

CREATE TABLE promo_codes (
    code                    TEXT PRIMARY KEY,
    discount_percent        INTEGER NOT NULL CHECK(discount_percent BETWEEN 0 AND 100),
    grants_instant_player   INTEGER NOT NULL DEFAULT 0,
    max_uses                INTEGER,
    times_used              INTEGER NOT NULL DEFAULT 0,
    active                  INTEGER NOT NULL DEFAULT 1,
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at              TEXT
);

CREATE INDEX idx_reg_season     ON registrations(season_id);
CREATE INDEX idx_reg_status     ON registrations(status);
CREATE INDEX idx_reg_eth        ON registrations(eth_address);
CREATE INDEX idx_reg_amount     ON registrations(amount_wei);
CREATE INDEX idx_reg_name       ON registrations(factorio_name);
CREATE INDEX idx_promo_active   ON promo_codes(active);

INSERT INTO seasons (id, status, started_at, ends_at, save_path)
VALUES (1, 'active', datetime('now'), datetime('now', '+7 days'), NULL);
