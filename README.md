## Twilight Remote Desktop

This project aims to build a open source remote desktop suitable for any uses including gaming.

It was [previously written in C++](https://github.com/jsonmona/twilight-cpp), but was rewritten in Rust to improve safety and fix threading-related bugs.

This project is currently in pre-alpha, so no prebuilt binaries are provided.


## Note

I'm busy serving the mandatory military service in South Korea, please expect delayed response times up to 6 weeks.

That also means I don't really have any time to work on this project. :(


## Building
The building process is done and tested in windows.
To start building you need to have [nasm](https://www.nasm.us), [cmake](https://cmake.org) and [rust](https://cmake.org) installed on your system.
In the project folder run the cargo command to build: ```cargo build```

After it builds successfully, go to the "target" folder and then into the "debug" folder, there will be the executables.

## Use
⚠️It only works with the local ip 127.0.0.1, since it is not yet ready for external ips.
There are 2 ways to use it:
1. If you want to do a quick debug, run the "debug" executable.
2. If you want to use both the server and the client, first run the server executable with the command: ```server.exe```
And for the client you can use: ```client.exe 127.0.0.1 --cleartext```
For more information run the command: ```client.exe --help```

## Contribute
Help contribute to this project by sending issues or pull requests, they are appreciated and you will help the project :)

It may take a long time to respond as the note says.

## License

Copyright (c) 2023. Yeomin Yoon. All rights reserved.

This project has no license attached yet.
That effectively makes the project somewhat like a closed-source one.
Of course, I plan to open source it.
I just need some time to research which license to apply.

I'm considering either GPLv3 or AGPLv3, with some additional clause to allow using vendor proprietary API like NVENC.
As far as I understand, their API is incompatible with GPL style license, so they need some additional clause.
I may be wrong, and that's why I hesitate to attach some license.
