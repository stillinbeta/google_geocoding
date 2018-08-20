//! A strongly-typed (a)synchronous Rusty API for the Google Geocoding API
//!
//! ## Synchronous API (Basic)
//!
//! The synchronous API is optimized for the ergonomics of common usage.
//!
//! You can do a simple look up of coordinates from an address:
//!
//! ```
//! use google_geocoding::geocode;
//! for coordinates in geocode("1600 Amphitheater Parkway, Mountain View, CA").unwrap() {
//!     println!("{}", coordinates);
//! }
//! ```
//!
//! Do a simple look up of an address from coordinates:
//!
//! ```
//! use google_geocoding::{WGS84, degeocode};
//! let coordinates = WGS84::try_new(37.42241, -122.08561, 0.0).unwrap();
//! for address in degeocode(coordinates).unwrap() {
//!     println!("{}", address);
//! }
//! ```
//!
//! Note that it is recommended to use WGS84::try_new() as WGS84::new() will panic
//! with invalid coordinates
//!
//! The synchronous API provides the address or coordinates from the API reply.
//! However, the full reply includes a great deal more information. For access to
//! the full reply, see the lowlevel asynchronous API.
//!
//! ## Synchronous API (Advanced)
//!
//! The GeocodeQuery and DegeocodeQuery objects can be used for more complex lookups
//!
//! ```
//! use google_geocoding::{GeocodeQuery, Language, Region, geocode};
//! let query = GeocodeQuery::new("1600 Amphitheater Parkway, Mountain View, CA")
//!     .language(Language::English)
//!     .region(Region::UnitedStates);
//! for coordinates in geocode(query).unwrap() {
//!     println!("{}", coordinates);
//! }
//! ```
//!
//! ## Asynchronous API
//!
//! The Connection object provides access to the lowlevel async-based API.
//!
//! Unlike the synchronous API, these functions provide the full API reply.
//! You will therefore need to extract the specific information you want.
//!
//! These functions are used to implement the Synchronous API described earlier.
//!
//! ```
//! extern crate google_geocoding;
//! extern crate tokio_core;
//!
//! use google_geocoding::Connection;
//! use tokio_core::reactor::Core;
//!
//! const ADDRESS: &str = "1600 Amphitheater Parkway, Mountain View, CA";
//!
//! let mut core = Core::new().unwrap();
//! let core_handle = core.handle();
//! let geocode_future = Connection::new(&core_handle).geocode(ADDRESS);
//! let reply = core.run(geocode_future).unwrap();
//!
//! for candidate in reply {
//!     println!("{}: {}", candidate.formatted_address, candidate.geometry.location);
//! }
//! ```
//!
//! ## Notes
//!
//! This is an unofficial library.
//!
//! [Official Google Geocoding API](https://developers.google.com/maps/documentation/geocoding/intro])
#![deny(missing_docs)]
#[macro_use]
extern crate failure;
extern crate futures;
extern crate itertools;
#[cfg(test)]
#[macro_use]
extern crate log;
extern crate nav_types;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate serde_urlencoded;
#[macro_use]
extern crate shrinkwraprs;
extern crate strum;
#[macro_use]
extern crate strum_macros;
extern crate tokio_core;
extern crate url;
mod serde_util;

use futures::{Future, Stream};

use failure::Error;
pub use nav_types::WGS84;
use reqwest::unstable::async::Client;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Eq;
use std::collections::HashSet;
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::Hash;
use tokio_core::reactor::Core;
use url::Url;

type Result<T> = std::result::Result<T, Error>;

/// One component of a separated address
#[derive(Debug, Deserialize, Serialize)]
pub struct AddressComponent {
    /// The full text description or name of the address component as returned by the Geocoder.
    long_name: String,
    /// An abbreviated textual name for the address component, if available.
    /// For example, an address component for the state of Alaska may have a long_name of "Alaska" and a short_name of "AK" using the 2-letter postal abbreviation.
    short_name: String,
    /// The type of the address component.
    types: Vec<Type>,
}

/// Position information
#[derive(Debug, Deserialize)]
pub struct Geometry {
    /// The geocoded latitude, longitude value.
    /// For normal address lookups, this field is typically the most important.
    pub location: Coordinates,
    /// Stores additional data about the specified location
    pub location_type: LocationType,
    /// the recommended viewport for displaying the returned result, specified as two latitude,longitude values defining the southwest and northeast corner of the viewport bounding box. Generally the viewport is used to frame a result when displaying it to a user.
    pub viewport: Viewport,
    /// The bounding box which can fully contain the returned result.
    /// Note that these bounds may not match the recommended viewport. (For example, San Francisco includes the Farallon islands, which are technically part of the city, but probably should not be returned in the viewport.)
    pub bounds: Option<Viewport>
}

/// What location Geometry refers to
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all="SCREAMING_SNAKE_CASE")]
pub enum LocationType {
    /// Indicates that the returned result is a precise geocode
    /// for which we have location information accurate down to street address precision.
    Rooftop,

    /// Indicates that the returned result reflects an approximation (usually on a road)
    /// interpolated between two precise points (such as intersections).
    /// Interpolated results are generally returned when rooftop geocodes
    /// are unavailable for a street address.
    RangeInterpolated,

    /// Indicates that the returned result is the geometric center of a result
    /// such as a polyline (for example, a street) or polygon (region).
    GeometricCenter,

    /// Indicates that the returned result is approximate.
    Approximate,
}

/// An API set that deseriaizes as a JSON array and serializes with pipe spaces
#[derive(Clone, Debug, Shrinkwrap)]
pub struct ApiSet<T>(HashSet<T>) where T: Eq + Hash + Serialize;

impl<'de,T> Deserialize<'de> for ApiSet<T>
    where T: Eq + Hash + Deserialize<'de> + Serialize {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error> where D: Deserializer<'de> {
        Ok(ApiSet(Vec::<T>::deserialize(deserializer)?.into_iter().collect()))
    }
}

impl<T> Serialize for ApiSet<T>
    where T: Eq + Hash + Serialize {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> where S: Serializer {
        use itertools::Itertools;
        serializer.serialize_str(&self.0.iter().map(serde_util::variant_name).join("|"))
    }
}

/// A human-readable address of this location.
#[derive(Debug,Deserialize)]
pub struct FormattedAddress(String);

impl Display for FormattedAddress {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// A reply from the Google geocoding API
#[derive(Debug, Deserialize)]
pub struct Reply {
    /// The separate components applicable to this address. 
    pub address_components: Vec<AddressComponent>,
    /// The human-readable address of this location.
    /// 
    /// Often this address is equivalent to the postal address. Note that some countries, such as the United Kingdom, do not allow distribution of true postal addresses due to licensing restrictions.
    ///
    /// The formatted address is logically composed of one or more address components. For example, the address "111 8th Avenue, New York, NY" consists of the following components: "111" (the street number), "8th Avenue" (the route), "New York" (the city) and "NY" (the US state).
    ///
    /// Do not parse the formatted address programmatically. Instead you should use the individual address components, which the API response includes in addition to the formatted address field.
    pub formatted_address: FormattedAddress,
    /// Position information
    pub geometry: Geometry,
    /// A unique identifier that can be used with other Google APIs.
    pub place_id: PlaceId,
    /// All the localities contained in a postal code.
    /// This is only present when the result is a postal code that contains multiple localities.
    pub postcode_localities: Option<Vec<String>>,

    /// The type of the returned result. This array contains a set of zero or more tags identifying the type of feature returned in the result. For example, a geocode of "Chicago" returns "locality" which indicates that "Chicago" is a city, and also returns "political" which indicates it is a political entity.
    pub types: Vec<Type>,
}

#[derive(Debug, Deserialize)]
struct ReplyResult {
    error_message: Option<String>,
    results: Vec<Reply>,
    status: StatusCode,
}

/// Status codes for the geocode API
#[derive(Debug, Deserialize, Fail)]
#[serde(rename_all="SCREAMING_SNAKE_CASE")]
pub enum StatusCode {
    /// Indicates that no errors occurred;
    /// the address was successfully parsed and at least one geocode was returned.
    #[fail(display="No errors occurred")]
    Ok,

    /// Indicates that the geocode was successful but returned no results.
    /// This may occur if the geocoder was passed a non-existent address.
    #[fail(display="Geocode was successful but returned no results.")]
    ZeroResults,

    /// Indicates that you are over your quota.
    #[fail(display="You are over your quota")]
    OverQueryLimit,

    /// Indicates that your request was denied
    #[fail(display="Request denied")]
    RequestDenied,

    /// generally indicates that the query (address, components or latlng) is missing.
    #[fail(display="Query component missing")]
    InvalidRequest,

    /// Indicates that the request could not be processed due to a server error.
    /// The request may succeed if you try again.
    #[fail(display="Unknown error")]
    UnknownError
}

/// The type of an address (eg street, intersection, etc)
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all="snake_case")]
pub enum Type {
    /// Indicates a precise street address.
    StreetAddress,

    /// Indicates a named route (such as "US 101").
    Route,

    /// Indicates a major intersection, usually of two major roads.
    Intersection,

    /// Indicates a political entity.
    /// Usually, this type indicates a polygon of some civil administration.
    Political,

    /// Indicates the national political entity,
    /// and is typically the highest order type returned by the Geocoder.
    Country,

    /// Indicates a first-order civil entity below the country level.
    /// Within the United States, these administrative levels are states.
    /// Not all nations exhibit these administrative levels.
    /// In most cases, administrative_area_level_1 short names will closely match
    /// ISO 3166-2 subdivisions and other widely circulated lists;
    /// however this is not guaranteed as our geocoding results are based
    /// on a variety of signals and location data.
    #[serde(rename="administrative_area_level_1")]
    AdministrativeAreaLevel1,

    /// Indicates a second-order civil entity below the country level.
    /// Within the United States, these administrative levels are counties.
    /// Not all nations exhibit these administrative levels.
    #[serde(rename="administrative_area_level_2")]
    AdministrativeAreaLevel2,

    /// Indicates a third-order civil entity below the country level.
    /// This type indicates a minor civil division.
    /// Not all nations exhibit these administrative levels.
    #[serde(rename="administrative_area_level_3")]
    AdministrativeAreaLevel3,

    /// Indicates a fourth-order civil entity below the country level.
    /// This type indicates a minor civil division.
    /// Not all nations exhibit these administrative levels.
    #[serde(rename="administrative_area_level_4")]
    AdministrativeAreaLevel4,

    /// Indicates a fifth-order civil entity below the country level.
    /// This type indicates a minor civil division.
    /// Not all nations exhibit these administrative levels.
    #[serde(rename="administrative_area_level_5")]
    AdministrativeAreaLevel5,

    /// Indicates a commonly-used alternative name for the entity.
    ColloquialArea,

    /// Indicates an incorporated city or town political entity.
    Locality,

    /// Indicates a specific type of Japanese locality,
    /// to facilitate distinction between multiple locality components within a Japanese address.
    Ward,

    /// Indicates a first-order civil entity below a locality.
    /// For some locations may receive one of the additional types:
    /// sublocality_level_1 to sublocality_level_5.
    /// Each sublocality level is a civil entity.
    /// Larger numbers indicate a smaller geographic area.
    Sublocality,

    /// Indicates a named neighborhood
    Neighborhood,

    /// Indicates a named location, usually a building or collection of buildings with a common name
    Premise,

    /// Indicates a first-order entity below a named location,
    /// usually a singular building within a collection of buildings with a common name
    Subpremise,

    /// Indicates a postal code as used to address postal mail within the country.
    PostalCode,

    /// Indicates a prominent natural feature.
    NaturalFeature,

    /// Indicates an airport.
    Airport,

    /// Indicates a named park.
    Park,

    /// Indicates a named point of interest.
    /// Typically, these "POI"s are prominent local entities
    /// that don't easily fit in another category, such as "Empire State Building"
    /// or "Statue of Liberty."
    PointOfInterest,

    /// Indicates the floor of a building address.
    Floor,

    /// Typically indicates a place that has not yet been categorized.
    Establishment,

    /// Indicates a parking lot or parking structure.
    Parking,

    /// Indicates a specific postal box.
    PostBox,

    /// Indicates a grouping of geographic areas, such as locality and sublocality,
    /// used for mailing addresses in some countries.
    PostalTown,

    /// Indicates the room of a building address.
    Room,

    /// Indicates the precise street number.
    StreetNumber,

    /// Indicate the location of a bus stop.
    BusStation,

    /// Indicate the location of a train station.
    TrainStation,

    /// Indicate the location of a public transit station.
    TransitStation,
}

/// A bounding box defined by northeast and southwest coordinates
#[derive(Clone,Copy,Debug,Deserialize,Serialize)]
pub struct Viewport {
    /// Northeast corner of the bounding box
    pub northeast: Coordinates,
    /// Southwest corner of the bounding box
    pub southwest: Coordinates,
}

/// Language that gets serialized as a language code
/// 
/// From https://developers.google.com/maps/faq#languagesupport
#[derive(Clone,Copy,Debug,EnumIter,Serialize)]
#[allow(dead_code)]
pub enum Language {
    /// Arabic (ar)
    #[serde(rename="ar")] Arabic,
    /// Bulgarian (bg)
    #[serde(rename="bg")] Bulgarian,
    /// Bengali (bn)
    #[serde(rename="bn")] Bengali,
    /// Catalan (ca)
    #[serde(rename="ca")] Catalan,
    /// Czech (cs)
    #[serde(rename="cs")] Czech,
    /// Danish (da)
    #[serde(rename="da")] Danish,
    /// German (de)
    #[serde(rename="de")] German,
    /// Greek (el)
    #[serde(rename="el")] Greek,
    /// English (en)
    #[serde(rename="en")] English,
    /// EnglishAustralian (en-AU)
    #[serde(rename="en-AU")] EnglishAustralian,
    /// EnglishGreatBritain (en-GB)
    #[serde(rename="en-GB")] EnglishGreatBritain,
    /// Spanish (es)
    #[serde(rename="es")] Spanish,
    /// Basque (eu)
    #[serde(rename="eu")] Basque,
    /// Farsi (fa)
    #[serde(rename="fa")] Farsi,
    /// Finnish (fi)
    #[serde(rename="fi")] Finnish,
    /// Filipino (fil)
    #[serde(rename="fil")] Filipino,
    /// French (fr)
    #[serde(rename="fr")] French,
    /// Galician (gl)
    #[serde(rename="gl")] Galician,
    /// Gujarati (gu)
    #[serde(rename="gu")] Gujarati,
    /// Hindi (hi)
    #[serde(rename="hi")] Hindi,
    /// Croatian (hr)
    #[serde(rename="hr")] Croatian,
    /// Hungarian (hu)
    #[serde(rename="hu")] Hungarian,
    /// Indonesian (id)
    #[serde(rename="id")] Indonesian,
    /// Italian (it)
    #[serde(rename="it")] Italian,
    /// Hebrew (iw)
    #[serde(rename="iw")] Hebrew,
    /// Japanese (ja)
    #[serde(rename="ja")] Japanese,
    /// Kannada (kn)
    #[serde(rename="kn")] Kannada,
    /// Korean (ko)
    #[serde(rename="ko")] Korean,
    /// Lithuanian (lt)
    #[serde(rename="lt")] Lithuanian,
    /// Latvian (lv)
    #[serde(rename="lv")] Latvian,
    /// Malayalam (ml)
    #[serde(rename="ml")] Malayalam,
    /// Marathi (mr)
    #[serde(rename="mr")] Marathi,
    /// Dutch (nl)
    #[serde(rename="nl")] Dutch,
    /// Norwegian (no)
    #[serde(rename="no")] Norwegian,
    /// Polish (pl)
    #[serde(rename="pl")] Polish,
    /// Portuguese (pt)
    #[serde(rename="pt")] Portuguese,
    /// PortugueseBrazil (pt-BR)
    #[serde(rename="pt-BR")] PortugueseBrazil,
    /// PortuguesePortugal (pt-PT)
    #[serde(rename="pt-PT")] PortuguesePortugal,
    /// Romanian (ro)
    #[serde(rename="ro")] Romanian,
    /// Russian (ru)
    #[serde(rename="ru")] Russian,
    /// Slovak (sk)
    #[serde(rename="sk")] Slovak,
    /// Slovenian (sl)
    #[serde(rename="sl")] Slovenian,
    /// Serbian (sr)
    #[serde(rename="sr")] Serbian,
    /// Swedish (sv)
    #[serde(rename="sv")] Swedish,
    /// Tamil (ta)
    #[serde(rename="ta")] Tamil,
    /// Telugu (te)
    #[serde(rename="te")] Telugu,
    /// Thai (th)
    #[serde(rename="th")] Thai,
    /// Tagalog (tl)
    #[serde(rename="tl")] Tagalog,
    /// Turkish (tr)
    #[serde(rename="tr")] Turkish,
    /// Ukrainian (uk)
    #[serde(rename="uk")] Ukrainian,
    /// Vietnamese (vi)
    #[serde(rename="vi")] Vietnamese,
    /// ChineseSimplified (zh-CN)
    #[serde(rename="zh-CN")] ChineseSimplified,
    /// ChineseTraditional (zh-TW)
    #[serde(rename="zh-TW")] ChineseTraditional,
}

/// Country Code Top-Level Domain
/// From https://icannwiki.org/Country_code_top-level_domain
#[derive(Clone,Copy,Debug,EnumIter,Serialize)]
#[allow(dead_code)]
pub enum Region {
    /// AscensionIsland (.ac)
    #[serde(rename=".ac")] AscensionIsland,
    /// Andorra (.ad)
    #[serde(rename=".ad")] Andorra,
    /// UnitedArabEmirates (.ae)
    #[serde(rename=".ae")] UnitedArabEmirates,
    /// Afghanistan (.af)
    #[serde(rename=".af")] Afghanistan,
    /// AntiguaAndBarbuda (.ag)
    #[serde(rename=".ag")] AntiguaAndBarbuda,
    /// Anguilla (.ai)
    #[serde(rename=".ai")] Anguilla,
    /// Albania (.al)
    #[serde(rename=".al")] Albania,
    /// Armenia (.am)
    #[serde(rename=".am")] Armenia,
    /// AntillesNetherlands (.an)
    #[serde(rename=".an")] AntillesNetherlands,
    /// Angola (.ao)
    #[serde(rename=".ao")] Angola,
    /// Antarctica (.aq)
    #[serde(rename=".aq")] Antarctica,
    /// Argentina (.ar)
    #[serde(rename=".ar")] Argentina,
    /// AmericanSamoa (.as)
    #[serde(rename=".as")] AmericanSamoa,
    /// Austria (.at)
    #[serde(rename=".at")] Austria,
    /// Australia (.au)
    #[serde(rename=".au")] Australia,
    /// Aruba (.aw)
    #[serde(rename=".aw")] Aruba,
    /// AlandIslands (.ax)
    #[serde(rename=".ax")] AlandIslands,
    /// Azerbaijan (.az)
    #[serde(rename=".az")] Azerbaijan,
    /// BosniaAndHerzegovina (.ba)
    #[serde(rename=".ba")] BosniaAndHerzegovina,
    /// Barbados (.bb)
    #[serde(rename=".bb")] Barbados,
    /// Bangladesh (.bd)
    #[serde(rename=".bd")] Bangladesh,
    /// Belgium (.be)
    #[serde(rename=".be")] Belgium,
    /// BurkinaFaso (.bf)
    #[serde(rename=".bf")] BurkinaFaso,
    /// Bulgaria (.bg)
    #[serde(rename=".bg")] Bulgaria,
    /// Bahrain (.bh)
    #[serde(rename=".bh")] Bahrain,
    /// Burundi (.bi)
    #[serde(rename=".bi")] Burundi,
    /// Benin (.bj)
    #[serde(rename=".bj")] Benin,
    /// SaintBarthelemy (.bl)
    #[serde(rename=".bl")] SaintBarthelemy,
    /// Bermuda (.bm)
    #[serde(rename=".bm")] Bermuda,
    /// BruneiDarussalam (.bn)
    #[serde(rename=".bn")] BruneiDarussalam,
    /// Bolivia (.bo)
    #[serde(rename=".bo")] Bolivia,
    /// Bonaire (.bq)
    #[serde(rename=".bq")] BonaireSintEustatiusAndSaba,
    /// Brazil (.br)
    #[serde(rename=".br")] Brazil,
    /// Bahamas (.bs)
    #[serde(rename=".bs")] Bahamas,
    /// Bhutan (.bt)
    #[serde(rename=".bt")] Bhutan,
    /// BouvetIsland (.bv)
    #[serde(rename=".bv")] BouvetIsland,
    /// Botswana (.bw)
    #[serde(rename=".bw")] Botswana,
    /// Belarus (.by)
    #[serde(rename=".by")] Belarus,
    /// Belize (.bz)
    #[serde(rename=".bz")] Belize,
    /// Canada (.ca)
    #[serde(rename=".ca")] Canada,
    /// CocosIslands (.cc)
    #[serde(rename=".cc")] CocosIslands,
    /// DemocraticRepublicOfTheCongo (.cd)
    #[serde(rename=".cd")] DemocraticRepublicOfTheCongo,
    /// CentralAfricanRepublic (.cf)
    #[serde(rename=".cf")] CentralAfricanRepublic,
    /// RepublicOfCongo (.cg)
    #[serde(rename=".cg")] RepublicOfCongo,
    /// Switzerland (.ch)
    #[serde(rename=".ch")] Switzerland,
    /// CoteDivoire (.ci)
    #[serde(rename=".ci")] CoteDivoire,
    /// CookIslands (.ck)
    #[serde(rename=".ck")] CookIslands,
    /// Chile (.cl)
    #[serde(rename=".cl")] Chile,
    /// Cameroon (.cm)
    #[serde(rename=".cm")] Cameroon,
    /// China (.cn)
    #[serde(rename=".cn")] China,
    /// Colombia (.co)
    #[serde(rename=".co")] Colombia,
    /// CostaRica (.cr)
    #[serde(rename=".cr")] CostaRica,
    /// Cuba (.cu)
    #[serde(rename=".cu")] Cuba,
    /// CapeVerde (.cv)
    #[serde(rename=".cv")] CapeVerde,
    /// Curacao (.cw)
    #[serde(rename=".cw")] Curacao,
    /// ChristmasIsland (.cx)
    #[serde(rename=".cx")] ChristmasIsland,
    /// Cyprus (.cy)
    #[serde(rename=".cy")] Cyprus,
    /// CzechRepublic (.cz)
    #[serde(rename=".cz")] CzechRepublic,
    /// Germany (.de)
    #[serde(rename=".de")] Germany,
    /// Djibouti (.dj)
    #[serde(rename=".dj")] Djibouti,
    /// Denmark (.dk)
    #[serde(rename=".dk")] Denmark,
    /// Dominica (.dm)
    #[serde(rename=".dm")] Dominica,
    /// DominicanRepublic (.do)
    #[serde(rename=".do")] DominicanRepublic,
    /// Algeria (.dz)
    #[serde(rename=".dz")] Algeria,
    /// Ecuador (.ec)
    #[serde(rename=".ec")] Ecuador,
    /// Estonia (.ee)
    #[serde(rename=".ee")] Estonia,
    /// Egypt (.eg)
    #[serde(rename=".eg")] Egypt,
    /// WesternSahara (.eh)
    #[serde(rename=".eh")] WesternSahara,
    /// Eritrea (.er)
    #[serde(rename=".er")] Eritrea,
    /// Spain (.es)
    #[serde(rename=".es")] Spain,
    /// Ethiopia (.et)
    #[serde(rename=".et")] Ethiopia,
    /// EuropeanUnion (.eu)
    #[serde(rename=".eu")] EuropeanUnion,
    /// Finland (.fi)
    #[serde(rename=".fi")] Finland,
    /// Fiji (.fj)
    #[serde(rename=".fj")] Fiji,
    /// FalklandIslands (.fk)
    #[serde(rename=".fk")] FalklandIslands,
    /// FederatedStatesOfMicronesia (.fm)
    #[serde(rename=".fm")] FederatedStatesOfMicronesia,
    /// FaroeIslands (.fo)
    #[serde(rename=".fo")] FaroeIslands,
    /// France (.fr)
    #[serde(rename=".fr")] France,
    /// Gabon (.ga)
    #[serde(rename=".ga")] Gabon,
    /// Grenada (.gd)
    #[serde(rename=".gd")] Grenada,
    /// Georgia (.ge)
    #[serde(rename=".ge")] Georgia,
    /// FrenchGuiana (.gf)
    #[serde(rename=".gf")] FrenchGuiana,
    /// Guernsey (.gg)
    #[serde(rename=".gg")] Guernsey,
    /// Ghana (.gh)
    #[serde(rename=".gh")] Ghana,
    /// Gibraltar (.gi)
    #[serde(rename=".gi")] Gibraltar,
    /// Greenland (.gl)
    #[serde(rename=".gl")] Greenland,
    /// Gambia (.gm)
    #[serde(rename=".gm")] Gambia,
    /// Guinea (.gn)
    #[serde(rename=".gn")] Guinea,
    /// Guadeloupe (.gp)
    #[serde(rename=".gp")] Guadeloupe,
    /// EquatorialGuinea (.gq)
    #[serde(rename=".gq")] EquatorialGuinea,
    /// Greece (.gr)
    #[serde(rename=".gr")] Greece,
    /// SouthGeorgiaAndTheSouthSandwichIslands (.gs)
    #[serde(rename=".gs")] SouthGeorgiaAndTheSouthSandwichIslands,
    /// Guatemala (.gt)
    #[serde(rename=".gt")] Guatemala,
    /// Guam (.gu)
    #[serde(rename=".gu")] Guam,
    /// GuineaBissau (.gw)
    #[serde(rename=".gw")] GuineaBissau,
    /// Guyana (.gy)
    #[serde(rename=".gy")] Guyana,
    /// HongKong (.hk)
    #[serde(rename=".hk")] HongKong,
    /// HeardIslandAndMcDonaldIslands (.hm)
    #[serde(rename=".hm")] HeardIslandAndMcDonaldIslands,
    /// Honduras (.hn)
    #[serde(rename=".hn")] Honduras,
    /// Croatia (.hr)
    #[serde(rename=".hr")] Croatia,
    /// Haiti (.ht)
    #[serde(rename=".ht")] Haiti,
    /// Hungary (.hu)
    #[serde(rename=".hu")] Hungary,
    /// Indonesia (.id)
    #[serde(rename=".id")] Indonesia,
    /// Ireland (.ie)
    #[serde(rename=".ie")] Ireland,
    /// Israel (.il)
    #[serde(rename=".il")] Israel,
    /// IsleOfMan (.im)
    #[serde(rename=".im")] IsleOfMan,
    /// India (.in)
    #[serde(rename=".in")] India,
    /// BritishIndianOceanTerritory (.io)
    #[serde(rename=".io")] BritishIndianOceanTerritory,
    /// Iraq (.iq)
    #[serde(rename=".iq")] Iraq,
    /// IslamicRepublicOfIran (.ir)
    #[serde(rename=".ir")] IslamicRepublicOfIran,
    /// Iceland (.is)
    #[serde(rename=".is")] Iceland,
    /// Italy (.it)
    #[serde(rename=".it")] Italy,
    /// Jersey (.je)
    #[serde(rename=".je")] Jersey,
    /// Jamaica (.jm)
    #[serde(rename=".jm")] Jamaica,
    /// Jordan (.jo)
    #[serde(rename=".jo")] Jordan,
    /// Japan (.jp)
    #[serde(rename=".jp")] Japan,
    /// Kenya (.ke)
    #[serde(rename=".ke")] Kenya,
    /// Kyrgyzstan (.kg)
    #[serde(rename=".kg")] Kyrgyzstan,
    /// Cambodia (.kh)
    #[serde(rename=".kh")] Cambodia,
    /// Kiribati (.ki)
    #[serde(rename=".ki")] Kiribati,
    /// Comoros (.km)
    #[serde(rename=".km")] Comoros,
    /// SaintKittsAndNevis (.kn)
    #[serde(rename=".kn")] SaintKittsAndNevis,
    /// DemocraticPeoplesRepublicOfKorea (.kp)
    #[serde(rename=".kp")] DemocraticPeoplesRepublicOfKorea,
    /// RepublicOfKorea (.kp)
    #[serde(rename=".kp")] RepublicOfKorea,
    /// Kuwait (.kw)
    #[serde(rename=".kw")] Kuwait,
    /// CaymenIslands (.ky)
    #[serde(rename=".ky")] CaymenIslands,
    /// Kazakhstan (.kz)
    #[serde(rename=".kz")] Kazakhstan,
    /// Laos (.la)
    #[serde(rename=".la")] Laos,
    /// Lebanon (.lb)
    #[serde(rename=".lb")] Lebanon,
    /// SaintLucia (.lc)
    #[serde(rename=".lc")] SaintLucia,
    /// Liechtenstein (.li)
    #[serde(rename=".li")] Liechtenstein,
    /// SriLanka (.lk)
    #[serde(rename=".lk")] SriLanka,
    /// Liberia (.lr)
    #[serde(rename=".lr")] Liberia,
    /// Lesotho (.ls)
    #[serde(rename=".ls")] Lesotho,
    /// Lithuania (.lt)
    #[serde(rename=".lt")] Lithuania,
    /// Luxembourg (.lu)
    #[serde(rename=".lu")] Luxembourg,
    /// Latvia (.lv)
    #[serde(rename=".lv")] Latvia,
    /// Libya (.ly)
    #[serde(rename=".ly")] Libya,
    /// Morocco (.ma)
    #[serde(rename=".ma")] Morocco,
    /// Monaco (.mc)
    #[serde(rename=".mc")] Monaco,
    /// RepublicOfMoldova (.md)
    #[serde(rename=".md")] RepublicOfMoldova,
    /// Montenegro (.me)
    #[serde(rename=".me")] Montenegro,
    /// SaintMartin (.mf)
    #[serde(rename=".mf")] SaintMartin,
    /// Madagascar (.mg)
    #[serde(rename=".mg")] Madagascar,
    /// MarshallIslands (.mh)
    #[serde(rename=".mh")] MarshallIslands,
    /// Macedonia (.mk)
    #[serde(rename=".mk")] Macedonia,
    /// Mali (.ml)
    #[serde(rename=".ml")] Mali,
    /// Myanmar (.mm)
    #[serde(rename=".mm")] Myanmar,
    /// Mongolia (.mn)
    #[serde(rename=".mn")] Mongolia,
    /// Macao (.mo)
    #[serde(rename=".mo")] Macao,
    /// NorthernMarianaIslands (.mp)
    #[serde(rename=".mp")] NorthernMarianaIslands,
    /// Martinique (.mq)
    #[serde(rename=".mq")] Martinique,
    /// Mauritania (.mr)
    #[serde(rename=".mr")] Mauritania,
    /// Montserrat (.ms)
    #[serde(rename=".ms")] Montserrat,
    /// Malta (.mt)
    #[serde(rename=".mt")] Malta,
    /// Mauritius (.mu)
    #[serde(rename=".mu")] Mauritius,
    /// Maldives (.mv)
    #[serde(rename=".mv")] Maldives,
    /// Malawi (.mw)
    #[serde(rename=".mw")] Malawi,
    /// Mexico (.mx)
    #[serde(rename=".mx")] Mexico,
    /// Malaysia (.my)
    #[serde(rename=".my")] Malaysia,
    /// Mozambique (.mz)
    #[serde(rename=".mz")] Mozambique,
    /// Namibia (.na)
    #[serde(rename=".na")] Namibia,
    /// NewCaledonia (.nc)
    #[serde(rename=".nc")] NewCaledonia,
    /// Niger (.ne)
    #[serde(rename=".ne")] Niger,
    /// NorfolkIsland (.nf)
    #[serde(rename=".nf")] NorfolkIsland,
    /// Nigeria (.ng)
    #[serde(rename=".ng")] Nigeria,
    /// Nicaragua (.ni)
    #[serde(rename=".ni")] Nicaragua,
    /// Netherlands (.nl)
    #[serde(rename=".nl")] Netherlands,
    /// Norway (.no)
    #[serde(rename=".no")] Norway,
    /// Nepal (.np)
    #[serde(rename=".np")] Nepal,
    /// Nauru (.nr)
    #[serde(rename=".nr")] Nauru,
    /// Niue (.nu)
    #[serde(rename=".nu")] Niue,
    /// NewZealand (.nz)
    #[serde(rename=".nz")] NewZealand,
    /// Oman (.om)
    #[serde(rename=".om")] Oman,
    /// Panama (.pa)
    #[serde(rename=".pa")] Panama,
    /// Peru (.pe)
    #[serde(rename=".pe")] Peru,
    /// FrenchPolynesia (.pf)
    #[serde(rename=".pf")] FrenchPolynesia,
    /// PapuaNewGuinea (.pg)
    #[serde(rename=".pg")] PapuaNewGuinea,
    /// Philippines (.ph)
    #[serde(rename=".ph")] Philippines,
    /// Pakistan (.pk)
    #[serde(rename=".pk")] Pakistan,
    /// Poland (.pl)
    #[serde(rename=".pl")] Poland,
    /// SaintPierreAndMiquelon (.pm)
    #[serde(rename=".pm")] SaintPierreAndMiquelon,
    /// Pitcairn (.pn)
    #[serde(rename=".pn")] Pitcairn,
    /// PuertoRico (.pr)
    #[serde(rename=".pr")] PuertoRico,
    /// Palestine (.ps)
    #[serde(rename=".ps")] Palestine,
    /// Portugal (.pt)
    #[serde(rename=".pt")] Portugal,
    /// Palau (.pw)
    #[serde(rename=".pw")] Palau,
    /// Paraguay (.py)
    #[serde(rename=".py")] Paraguay,
    /// Qatar (.qa)
    #[serde(rename=".qa")] Qatar,
    /// Reunion (.re)
    #[serde(rename=".re")] Reunion,
    /// Romania (.ro)
    #[serde(rename=".ro")] Romania,
    /// Serbia (.rs)
    #[serde(rename=".rs")] Serbia,
    /// Russia (.ru)
    #[serde(rename=".ru")] Russia,
    /// Rwanda (.rw)
    #[serde(rename=".rw")] Rwanda,
    /// SaudiArabia (.sa)
    #[serde(rename=".sa")] SaudiArabia,
    /// SolomonIslands (.sb)
    #[serde(rename=".sb")] SolomonIslands,
    /// Seychelles (.sc)
    #[serde(rename=".sc")] Seychelles,
    /// Sudan (.sd)
    #[serde(rename=".sd")] Sudan,
    /// Sweden (.se)
    #[serde(rename=".se")] Sweden,
    /// Singapore (.sg)
    #[serde(rename=".sg")] Singapore,
    /// SaintHelena (.sh)
    #[serde(rename=".sh")] SaintHelena,
    /// Slovenia (.si)
    #[serde(rename=".si")] Slovenia,
    /// SvalbardAndJanMayen (.sj)
    #[serde(rename=".sj")] SvalbardAndJanMayen,
    /// Slovakia (.sk)
    #[serde(rename=".sk")] Slovakia,
    /// SierraLeone (.sl)
    #[serde(rename=".sl")] SierraLeone,
    /// SanMarino (.sm)
    #[serde(rename=".sm")] SanMarino,
    /// Senegal (.sn)
    #[serde(rename=".sn")] Senegal,
    /// Somalia (.so)
    #[serde(rename=".so")] Somalia,
    /// Suriname (.sr)
    #[serde(rename=".sr")] Suriname,
    /// SouthSudan (.ss)
    #[serde(rename=".ss")] SouthSudan,
    /// SaoTomeAndPrincipe (.st)
    #[serde(rename=".st")] SaoTomeAndPrincipe,
    /// SovietUnion (.su)
    #[serde(rename=".su")] SovietUnion,
    /// ElSalvador (.sv)
    #[serde(rename=".sv")] ElSalvador,
    /// SintMaarten (.sx)
    #[serde(rename=".sx")] SintMaarten,
    /// Syria (.sy)
    #[serde(rename=".sy")] Syria,
    /// Swaziland (.sz)
    #[serde(rename=".sz")] Swaziland,
    /// TurksAndCaicosIslands (.tc)
    #[serde(rename=".tc")] TurksAndCaicosIslands,
    /// Chad (.td)
    #[serde(rename=".td")] Chad,
    /// FrenchSouthernTerritories (.tf)
    #[serde(rename=".tf")] FrenchSouthernTerritories,
    /// Togo (.tg)
    #[serde(rename=".tg")] Togo,
    /// Thailand (.th)
    #[serde(rename=".th")] Thailand,
    /// Tajikistan (.tj)
    #[serde(rename=".tj")] Tajikistan,
    /// Tokelau (.tk)
    #[serde(rename=".tk")] Tokelau,
    /// TimorLeste (.tl)
    #[serde(rename=".tl")] TimorLeste,
    /// Turkmenistan (.tm)
    #[serde(rename=".tm")] Turkmenistan,
    /// Tunisia (.tn)
    #[serde(rename=".tn")] Tunisia,
    /// Tonga (.to)
    #[serde(rename=".to")] Tonga,
    /// PortugueseTimor (.tp)
    #[serde(rename=".tp")] PortugueseTimor,
    /// Turkey (.tr)
    #[serde(rename=".tr")] Turkey,
    /// TrinidadAndTobago (.tt)
    #[serde(rename=".tt")] TrinidadAndTobago,
    /// Tuvalu (.tv)
    #[serde(rename=".tv")] Tuvalu,
    /// Taiwan (.tw)
    #[serde(rename=".tw")] Taiwan,
    /// Tanzania (.tz)
    #[serde(rename=".tz")] Tanzania,
    /// Ukraine (.ua)
    #[serde(rename=".ua")] Ukraine,
    /// Uganda (.ug)
    #[serde(rename=".ug")] Uganda,
    /// UnitedKingdom (.uk)
    #[serde(rename=".uk")] UnitedKingdom,
    /// UnitedStatesMinorOutlyingIslands (.um)
    #[serde(rename=".um")] UnitedStatesMinorOutlyingIslands,
    /// UnitedStates (.us)
    #[serde(rename=".us")] UnitedStates,
    /// Uruguay (.uy)
    #[serde(rename=".uy")] Uruguay,
    /// Uzbekistan (.uz)
    #[serde(rename=".uz")] Uzbekistan,
    /// VaticanCity (.va)
    #[serde(rename=".va")] VaticanCity,
    /// SaintVincentAndTheGrenadines (.vc)
    #[serde(rename=".vc")] SaintVincentAndTheGrenadines,
    /// Venezuela (.ve)
    #[serde(rename=".ve")] Venezuela,
    /// BritishVirginIslands (.vg)
    #[serde(rename=".vg")] BritishVirginIslands,
    /// USVirginIslands (.vi)
    #[serde(rename=".vi")] USVirginIslands,
    /// Vietnam (.vn)
    #[serde(rename=".vn")] Vietnam,
    /// Vanuatu (.vu)
    #[serde(rename=".vu")] Vanuatu,
    /// WallisAndFutuna (.wf)
    #[serde(rename=".wf")] WallisAndFutuna,
    /// Samoa (.ws)
    #[serde(rename=".ws")] Samoa,
    /// Mayote (.yt)
    #[serde(rename=".yt")] Mayote,
    /// SouthAfrica (.za)
    #[serde(rename=".za")] SouthAfrica,
    /// Zambia (.zm)
    #[serde(rename=".zm")] Zambia,
    /// Zimbabwe (.zw)
    #[serde(rename=".zw")] Zimbabwe,
}

/// A rule for a component filter
#[derive(Debug,Eq,Hash,PartialEq)]
pub enum ComponentFilterRule {
    /// Matches postal_code and postal_code_prefix.
    PostalCode(String),
    /// Matches a country name or a two letter ISO 3166-1 country code. Note: The API follows the ISO standard for defining countries, and the filtering works best when using the corresponding ISO code of the country.
    Country(String),
    /// Matches the long or short name of a route.
    Route(String),
    /// Matches matches against locality and sublocality types.
    Locality(String),
    /// Matches all the administrative_area levels.
    AdministrativeArea(String),
}

impl Serialize for ComponentFilterRule {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> where S: Serializer {
        let v = match self {
            ComponentFilterRule::PostalCode(x)=>x,
            ComponentFilterRule::Country(x)=>x,
            ComponentFilterRule::Route(x)=>x,
            ComponentFilterRule::Locality(x)=>x,
            ComponentFilterRule::AdministrativeArea(x)=>x,
        };
        serializer.serialize_str(&format!("{}:{}", serde_util::variant_name(self), v))
    }
}

pub(crate) trait ApiQuery : Debug + Serialize {
}

/// Represents a connection to the Google geocoding API
pub struct Connection {
    client: Client,
}

impl Connection {
    const URL: &'static str = "https://maps.google.com/maps/api/geocode/json";

    /// Creates a new connection for the Google geocoding API on the specified reactor
    pub fn new(handle: &tokio_core::reactor::Handle) -> Self {
        Self {
            client: Client::new(handle)
        }
    }

    /// Get the address of the specified coordinates
    pub fn degeocode(&self, coordinates: impl Into<DegeocodeQuery>) -> impl Future<Item = Vec<Reply>, Error = Error> {
        self.get(coordinates.into())
    }

    /// Get the coordinates of the specified address
    pub fn geocode(&self, address: impl Into<GeocodeQuery>) -> impl Future<Item = Vec<Reply>, Error = Error> {
        self.get(address.into())
    }

    /// Perform the specified query
    fn get(&self, i_params: impl ApiQuery) -> impl Future<Item = Vec<Reply>, Error = Error> {
        // FIXME: unwrap below
        let mut url_full = Url::parse(Self::URL).unwrap();
        url_full.set_query(Some(serde_urlencoded::to_string(i_params).unwrap().as_ref()));
        self.client
            .get(url_full)
            .send()
            .map_err(Error::from)
            .and_then(move |res| res.into_body().concat2()
            .map_err(Error::from))
            .and_then(move |body| serde_json::from_slice(&body)
            .map_err(Error::from))
            .and_then(move |reply| {
                match reply {
                    ReplyResult { status: StatusCode::Ok, results, .. } => Ok(results),
                    ReplyResult { status: e, .. }  => Err(e.into()),
                }
            })
    }
}

/// WGS-84 coordinates that support serializing and deserializing
#[derive(Clone,Copy,Debug,Shrinkwrap)]
pub struct Coordinates(WGS84<f64>);

impl<'de> serde::Deserialize<'de> for Coordinates {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error> 
    where D: Deserializer<'de> {
        #[derive(Deserialize)]
        pub struct Helper {
            #[serde(rename="lat")]
            latitude: f64,
            #[serde(rename="lng")]
            longitude: f64,
        }
        Helper::deserialize(deserializer)
            .and_then(|x|WGS84::try_new(x.latitude, x.longitude, 0f64).ok_or(serde::de::Error::custom(format!("Coordinates ({},{}) do not lie on WGS-84 ellipsoid", x.latitude, x.longitude))))
            .map(|x|Coordinates(x))
    }
}

impl Serialize for Coordinates {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> where S: Serializer {
        serializer.serialize_str(&self.to_string())
    }
}

impl std::fmt::Display for Coordinates {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{},{}", self.0.latitude_degrees(), self.0.longitude_degrees())
    }
}

impl From<WGS84<f64>> for Coordinates {
    fn from(v: WGS84<f64>) -> Self {
        Coordinates(v)
    }
}


#[derive(Debug, Serialize)]
/// A query for an address
pub struct DegeocodeQuery {
    /// The latitude and longitude values specifying the location for which you wish to obtain the closest, human-readable address.
    #[serde(rename="latlng")]
    coordinates: Coordinates,

    /// The language in which to return results.
    language: Option<Language>,

    /// A filter of one or more address types.
    ///
    /// If the parameter contains multiple address types, the API returns all addresses that match any of the types.
    ///
    /// A note about processing: The result_type parameter does not restrict the search to the specified address type(s). Rather, the result_type acts as a post-search filter: the API fetches all results for the specified latlng, then discards those results that do not match the specified address type(s).
    ///
    /// Note: This parameter is available only for requests that include an API key or a client ID.
    result_type: Option<ApiSet<Type>>,

    /// A filter of one or more location types.
    ///
    /// If the parameter contains multiple location types, the API returns all addresses that match any of the types.
    ///
    /// A note about processing: The location_type parameter does not restrict the search to the specified location type(s). Rather, the location_type acts as a post-search filter: the API fetches all results for the specified latlng, then discards those results that do not match the specified location type(s).
    ///
    /// Note: This parameter is available only for requests that include an API key or a client ID.
    location_type: Option<ApiSet<LocationType>>,
}

impl DegeocodeQuery {
    /// Creates a new address query
    pub fn new(coordinates: impl Into<Coordinates>) -> Self {
        DegeocodeQuery {
            coordinates: coordinates.into(),
            language: None,
            location_type: None,
            result_type: None,
        }
    }

    /// The language in which to return results.
    pub fn language(mut self, i_language: Language) -> Self {
        self.language = Some(i_language);
        self
    }

    /// A filter of one or more location types.
    ///
    /// If the parameter contains multiple location types, the API returns all addresses that match any of the types.
    ///
    /// A note about processing: The location_type parameter does not restrict the search to the specified location type(s). Rather, the location_type acts as a post-search filter: the API fetches all results for the specified latlng, then discards those results that do not match the specified location type(s).
    ///
    /// Note: This parameter is available only for requests that include an API key or a client ID.
    pub fn location_type(mut self, i_location_type: ApiSet<LocationType>) -> Self {
        self.location_type = Some(i_location_type);
        self
    }

    /// A filter of one or more address types.
    ///
    /// If the parameter contains multiple address types, the API returns all addresses that match any of the types.
    ///
    /// A note about processing: The result_type parameter does not restrict the search to the specified address type(s). Rather, the result_type acts as a post-search filter: the API fetches all results for the specified latlng, then discards those results that do not match the specified address type(s).
    ///
    /// Note: This parameter is available only for requests that include an API key or a client ID.
    pub fn result_type(mut self, i_result_type: ApiSet<Type>) -> Self {
        self.result_type = Some(i_result_type);
        self
    }
}

impl<T> From<T> for DegeocodeQuery where Coordinates: From<T> {
    fn from(v: T) -> Self {
        Self::new(v)
    }
}

impl ApiQuery for DegeocodeQuery{}

/// A query for coordinates
#[derive(Debug, Serialize)]
pub struct GeocodeQuery {
    #[serde(flatten)]
    filter: Option<Place>,

    /// The bounding box of the viewport within which to bias geocode results more prominently.
    /// This parameter will only influence, not fully restrict, results from the geocoder.
    /// (For more information see Viewport Biasing below.)
    bounds: Option<Viewport>,

    /// The language in which to return results.
    language: Option<Language>,

    /// The region code.
    ///
    /// This parameter will only influence, not fully restrict, results from the geocoder.
    /// (For more information see Region Biasing below.)
    region: Option<Region>,
}

impl GeocodeQuery {
    /// Creates a new coordinates query
    pub fn new(filter: impl Into<Place>) -> Self {
        GeocodeQuery {
            filter: Some(filter.into()),
            //components: None,
            bounds: None,
            language: None,
            region: None,
        }
    }

    /// The bounding box of the viewport within which to bias geocode results more prominently.
    /// This parameter will only influence, not fully restrict, results from the geocoder.
    /// (For more information see Viewport Biasing below.)
    pub fn bounds(mut self, i_bounds: Viewport) -> Self {
        self.bounds = Some(i_bounds);
        self
    }

    /// The language in which to return results.
    pub fn language(mut self, i_language: Language) -> Self {
        self.language = Some(i_language);
        self
    }

    /// The region code.
    ///
    /// This parameter will only influence, not fully restrict, results from the geocoder.
    /// (For more information see Region Biasing below.)
    pub fn region(mut self, i_region: Region) -> Self {
        self.region = Some(i_region);
        self
    }
}

impl ApiQuery for GeocodeQuery{}

impl<T> From<T> for GeocodeQuery where Place: From<T> {
    fn from(v: T) -> Self {
        Self::new(v)
    }
}

/// An address in one of various formats
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Place {
    /// A specific place
    Address {
        /// The street address that you want to geocode,
        /// in the format used by the national postal service of the country concerned.
        /// Additional address elements such as business names and unit, suite or floor numbers should be avoided.
        /// Please refer to the FAQ for additional guidance. 
        address: String
    },
    /// The address broken down into components
    ComponentFilter {
        /// The components filter is required if the request doesn't include an address.
        /// Each element in the components filter consists of a component:value pair,
        /// and fully restricts the results from the geocoder.
        /// See more information about component filtering below.
        components: ApiSet<ComponentFilterRule>
    },
}

impl<T> From<T> for Place where T: Into<String> {
    fn from(s: T) -> Self {
        Place::Address { 
            address: s.into()
        }
    }
}

/// A unique identifier that can be used with other Google APIs.
/// For example, you can use the place_id in a Places SDK request to get details of a local business, such as phone number, opening hours, user reviews, and more. See the place ID overview.
#[derive(Debug,Deserialize,Eq,Hash,PartialEq,Serialize)]
pub struct PlaceId(String);

/// Get all the coordinates associated with the specified filter
pub fn geocode(address: impl Into<GeocodeQuery>) -> Result<impl Iterator<Item=Coordinates>> {
    let mut core = Core::new()?;
    let core_handle = core.handle();
    Ok(core.run(Connection::new(&core_handle).geocode(address))?.into_iter().map(|x|x.geometry.location))
}

/// Get all the addresses associated with the specified coordinates
pub fn degeocode(coordinates: impl Into<DegeocodeQuery>) -> Result<impl Iterator<Item=FormattedAddress>> {
    let mut core = Core::new()?;
    let core_handle = core.handle();
    Ok(core.run(Connection::new(&core_handle).degeocode(coordinates))?.into_iter().map(|x|x.formatted_address))
}

#[cfg(test)]
mod test {
    use super::*;

    const ADDRESS: &str = "1600 Amphitheatre Pkwy, Mountain View, CA 94043, USA";
    const COORDINATES: (f64, f64) = (37.42241, -122.08561);

    fn test_print<L>(i_label: L, i_rr: impl Future<Item = Vec<Reply>, Error=Error>) -> impl Future<Item=(),Error=()> 
        where L: Debug + 'static {
        let label = i_label;
        i_rr
            .map(move |rr| println!("{:#?}: {:#?}", label, rr.iter().map(|r|&r.geometry.location).collect::<Vec<_>>()))
            .map_err(|e| error!("{:?}", e))

    }

    fn test_start() -> (Core, Connection) {
        let core = Core::new().expect("Failed to initialize core");
        let connection = Connection::new(&core.handle());

        (core, connection)
    }

    fn test_stop<T>(mut i_core: Core, i_future: T)
        where T: futures::Future<Item=(), Error=()> {
        i_core.run(i_future).expect("Failed running tests")
    }

    #[test]
    fn address() {
        super::degeocode(WGS84::try_new(COORDINATES.0, COORDINATES.1, 0.0).unwrap()).unwrap();
    }

    #[test]
    fn coordinates() {
        super::geocode(ADDRESS).unwrap();
    }

    #[test]
    fn connection_both() {
        let (core, connection) = test_start();
        let tests = futures::Future::join(
            test_print("Basic", connection.geocode(ADDRESS)),
            test_print("Basic", connection.degeocode(
                WGS84::new(COORDINATES.0, COORDINATES.1, 0f64)
            )),
        ).map(|_|());
        test_stop(core, tests)
    }

    #[test]
    fn compare() {
        let mut coordinates = super::geocode(ADDRESS).unwrap();
        let first_coordinates = coordinates.next().unwrap();
        let mut addresses = super::degeocode(first_coordinates).unwrap();
        let first_address = addresses.next().unwrap();
        assert_eq!(ADDRESS, first_address.0);
    }

    #[test]
    fn connection_address() {
        let (core, connection) = test_start();
        let tests = test_print("address", connection.degeocode(WGS84::try_new(COORDINATES.0, COORDINATES.1, 0.0).unwrap()));
        test_stop(core, tests)
    }

    #[test]
    fn connection_coordinates() {
        let (core, connection) = test_start();
        let tests = test_print("coordinates", connection.geocode(ADDRESS));
        test_stop(core, tests)
    }

    /*
    #[test]
    fn languages() {
        let (core, connection) = test_start();

        let tests = futures::future::join_all(Language::iter().take(10).map(|l| {
            test_print(l, connection.get(GeocodeQuery::new(ADDRESS).language(l)))
        })).map(|_|());

        test_stop(core, tests)
    }

    #[test]
    fn regions() {
        let (core, connection) = test_start();

        let tests = futures::future::join_all(Region::iter().map(|cc| {
            test_print(cc, connection.get(GeocodeQuery::from_address(ADDRESS).region(cc)))
        })).map(|_|());

        test_stop(core, tests)
    }
    */
}
