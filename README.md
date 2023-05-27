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

In the project folder run the cargo command to build: ```cargo build```.

After it builds successfully, go to the "target" folder and then into the "debug" folder, there will be the executables.

## Use
⚠️It only works with the local IP 127.0.0.1, since it is not yet ready for external IP's.
There are 2 ways to use it:
1. If you want to do a quick debug, run the "debug" executable.
2. If you want to use both the server and the client, first run the server executable with the command: ```server.exe```.
And for the client you can use: ```client.exe 127.0.0.1 --cleartext```.

For more information run the command: ```client.exe --help```.



## Contribute
Help contribute to this project by sending issues or pull requests, they are appreciated and you will help the project. :)

It may take a long time to respond as the note says.

## License

This project is licensed under GPLv3 or (at your option) any later version of GPL released by Free Software Foundation.

A copy of GPLv3 license is available at file `LICENSE.txt`.
You may visit https://www.gnu.org/licenses/ to find license text of GPLv3 or any later versions.

As far as I understand, This project might need some additional clause to allow using vendor proprietary API like NVENC.
The reason being that their API (or header file?) is incompatible with GPL style license, so they need some additional clause.
If this turns out to be true, the project might undergo a re-licensing process.
I'm not sure yet and has attached vanilla GPLv3 license for now.

SPDX-License-Idenfitifer: GPL-3.0-or-later
