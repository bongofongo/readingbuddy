PRAGMA user_version=20221111;

CREATE TABLE IF NOT EXISTS book
    (
        id integer PRIMARY KEY autoincrement,
        title text,
        authors text,
        notes      integer,
        last_open  integer,
        highlights integer,
        pages      integer,
        series text,
        language text,
        md5 text,
        total_read_time  integer,
        total_read_pages integer
    );

CREATE TABLE IF NOT EXISTS page_stat_data
    (
        id_book     integer,
        page        integer NOT NULL DEFAULT 0,
        start_time  integer NOT NULL DEFAULT 0,
        duration    integer NOT NULL DEFAULT 0,
        total_pages integer NOT NULL DEFAULT 0,
        UNIQUE (id_book, page, start_time),
        FOREIGN KEY(id_book) REFERENCES book(id)
    );

CREATE TABLE IF NOT EXISTS numbers
    (
        number INTEGER PRIMARY KEY
    );

CREATE UNIQUE INDEX IF NOT EXISTS book_title_authors_md5 ON book(title, authors, md5);
CREATE INDEX IF NOT EXISTS page_stat_data_start_time ON page_stat_data(start_time);

INSERT INTO book (id, title, authors, md5, pages, language) VALUES (1, 'Pachinko', 'A. Writer', '8cb32bca81b36ca0816851073e5661d3', 300, 'en');
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (1, 10, 1767574857, 300, 300);
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (1, 11, 1767575752, 240, 300);
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (1, 12, 1767576618, 180, 300);
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (1, 13, 1767661218, 600, 300);
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (1, 13, 1767662139, 120, 300);
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (1, 14, 1767663022, 90, 300);
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (1, 15, 1767920410, 3600, 300);
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (1, 99, 0, 500, 300);
INSERT INTO book (id, title, authors, md5, pages, language) VALUES (2, 'Brief Encounter', 'A. Writer', 'a5b01da92a68bbbb6d88c12483cf3b56', 300, 'en');
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (2, 1, 1767747621, 19, 300);
INSERT INTO book (id, title, authors, md5, pages, language) VALUES (3, 'A Book We Never Imported', 'A. Writer', '25dc3d7e5bd746db64267cff902d3edd', 300, 'en');
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (3, 3, 1767574812, 900, 300);
INSERT INTO book (id, title, authors, md5, pages, language) VALUES (4, 'No Checksum Here', 'A. Writer', NULL, 300, 'en');
INSERT INTO page_stat_data (id_book, page, start_time, duration, total_pages) VALUES (4, 2, 1767661232, 450, 300);
