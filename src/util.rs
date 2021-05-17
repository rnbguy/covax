pub fn lat_long_to_km(lat1: f64, long1: f64, lat2: f64, long2: f64) -> f64 {
    // Radius of the earth: 6371 km

    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();
    let long1 = long1.to_radians();
    let long2 = long2.to_radians();

    let a = ((lat2 - lat1) / 2.).sin().powi(2) + ((long2 - long1) / 2.).sin().powi(2);

    6371. * 2. * a.sqrt()
}
