@0xff07599f86310fab;

struct Result(T) {
    union {
        value @0 :T;
        error @1 :Error;
    }
}

struct Error {
    message @0 :Text;
}
