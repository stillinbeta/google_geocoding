# google_geocoding

A strongly-typed (a)synchronous Rusty API for the Google Geocoding API

### Synchronous API (Basic)

The synchronous API is optimized for the ergonomics of common usage.

You can do a simple look up of coordinates from an address:

```rust
use google_geocoding::geocode;
for coordinates in geocode("1600 Amphitheater Parkway, Mountain View, CA").unwrap() {
    println!("{}", coordinates);
}
```

Do a simple look up of an address from coordinates:

```rust
use google_geocoding::{WGS84, degeocode};
let coordinates = WGS84::try_new(37.42241, -122.08561, 0.0).unwrap();
for address in degeocode(coordinates).unwrap() {
    println!("{}", address);
}
```

Note that it is recommended to use WGS84::try_new() as WGS84::new() will panic
with invalid coordinates

The synchronous API provides the address or coordinates from the API reply.
However, the full reply includes a great deal more information. For access to
the full reply, see the lowlevel asynchronous API.

### Synchronous API (Advanced)

The GeocodeQuery and DegeocodeQuery objects can be used for more complex lookups

```rust
use google_geocoding::{GeocodeQuery, Language, Region, geocode};
let query = GeocodeQuery::new("1600 Amphitheater Parkway, Mountain View, CA")
    .language(Language::English)
    .region(Region::UnitedStates);
for coordinates in geocode(query).unwrap() {
    println!("{}", coordinates);
}
```

### Asynchronous API

The Connection object provides access to the lowlevel async-based API.

Unlike the synchronous API, these functions provide the full API reply.
You will therefore need to extract the specific information you want.

These functions are used to implement the Synchronous API described earlier.

```rust
extern crate google_geocoding;
extern crate tokio_core;

use google_geocoding::Connection;
use tokio_core::reactor::Core;

const ADDRESS: &str = "1600 Amphitheater Parkway, Mountain View, CA";

let mut core = Core::new().unwrap();
let core_handle = core.handle();
let geocode_future = Connection::new(&core_handle).geocode(ADDRESS);
let reply = core.run(geocode_future).unwrap();

for candidate in reply {
    println!("{}: {}", candidate.formatted_address, candidate.geometry.location);
}
```

### Notes

This is an unofficial library.

[Official Google Geocoding API](https://developers.google.com/maps/documentation/geocoding/intro])

License: MIT
