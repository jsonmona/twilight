@0xc7abb4188a2ffcd3;

struct DisplayFrame {
    keyframe @0 :Bool;      # True if keyframe (IDR frame)
    data @1 :Data;          # Data of the encoded frame
}
