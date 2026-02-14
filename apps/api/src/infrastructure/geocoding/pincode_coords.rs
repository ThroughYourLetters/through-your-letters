use lazy_static::lazy_static;
use std::collections::HashMap;

/// Default coordinates: Bangalore center (longitude, latitude)
const DEFAULT_COORDS: (f64, f64) = (77.5946, 12.9716);

lazy_static! {
    static ref PINCODE_MAP: HashMap<&'static str, (f64, f64)> = {
        let mut m = HashMap::new();
        // Bengaluru PIN codes mapped to approximate (longitude, latitude)
        m.insert("560001", (77.5946, 12.9716));  // GPO, MG Road
        m.insert("560002", (77.5750, 12.9850));  // Rajajinagar
        m.insert("560003", (77.5700, 12.9900));  // Basaveshwaranagar
        m.insert("560004", (77.5670, 12.9620));  // Chamrajpet
        m.insert("560005", (77.5430, 12.9580));  // Vijayanagar
        m.insert("560006", (77.5550, 12.9480));  // Hanumanthnagar
        m.insert("560007", (77.6200, 12.9580));  // Frazer Town
        m.insert("560008", (77.6000, 12.9820));  // Shivajinagar
        m.insert("560009", (77.5900, 12.9550));  // Richmond Town
        m.insert("560010", (77.5650, 12.9350));  // Basavanagudi
        m.insert("560011", (77.5800, 12.9450));  // Jayanagar
        m.insert("560012", (77.5860, 12.9320));  // Jayanagar East
        m.insert("560013", (77.5580, 12.9200));  // Yediyur
        m.insert("560014", (77.6050, 12.9250));  // Wilson Garden
        m.insert("560015", (77.5500, 12.9750));  // Mahalakshmi Layout
        m.insert("560016", (77.5350, 12.9700));  // Nandini Layout
        m.insert("560017", (77.6200, 12.9900));  // Benson Town
        m.insert("560018", (77.6350, 12.9750));  // Cox Town
        m.insert("560019", (77.6450, 12.9550));  // Ulsoor
        m.insert("560020", (77.5550, 13.0050));  // Malleshwaram
        m.insert("560021", (77.5700, 13.0100));  // Sadashivanagar
        m.insert("560022", (77.5400, 13.0200));  // Yeshwanthpur
        m.insert("560023", (77.5900, 13.0000));  // Seshadripuram
        m.insert("560024", (77.5650, 12.9150));  // Banashankari
        m.insert("560025", (77.6100, 12.9500));  // Adugodi
        m.insert("560026", (77.5800, 12.9100));  // Padmanabhanagar
        m.insert("560027", (77.6100, 12.9650));  // Shanthinagar
        m.insert("560028", (77.6300, 12.9350));  // Koramangala
        m.insert("560029", (77.5850, 12.9000));  // Banashankari 3rd Stage
        m.insert("560030", (77.5700, 12.8900));  // Uttarahalli
        m.insert("560031", (77.6500, 12.9450));  // Jogupalya
        m.insert("560032", (77.5500, 13.0350));  // RMV Extension
        m.insert("560033", (77.6200, 12.9100));  // HSR Layout
        m.insert("560034", (77.6400, 12.9200));  // BTM Layout
        m.insert("560035", (77.5300, 12.9500));  // Rajarajeshwari Nagar
        m.insert("560036", (77.6050, 12.9900));  // RT Nagar
        m.insert("560037", (77.6250, 12.8900));  // Madiwala
        m.insert("560038", (77.6550, 12.9700));  // Indiranagar
        m.insert("560039", (77.5200, 13.0000));  // Rajajinagar Industrial Town
        m.insert("560040", (77.5900, 13.0150));  // Sadashivanagar
        m.insert("560041", (77.6700, 12.9600));  // HAL
        m.insert("560042", (77.5650, 12.8800));  // Kumaraswamy Layout
        m.insert("560043", (77.5350, 13.0400));  // Mathikere
        m.insert("560044", (77.5150, 13.0100));  // Peenya
        m.insert("560045", (77.6500, 12.9850));  // Kacharakanahalli
        m.insert("560046", (77.6300, 13.0000));  // Ganganagar
        m.insert("560047", (77.5800, 13.0300));  // Hebbal
        m.insert("560048", (77.5950, 12.8800));  // JP Nagar
        m.insert("560049", (77.5400, 12.8950));  // Kengeri
        m.insert("560050", (77.6000, 13.0050));  // Palace Guttahalli
        m.insert("560051", (77.5500, 13.0500));  // Jalahalli
        m.insert("560052", (77.5300, 13.0600));  // Vidyaranyapura
        m.insert("560053", (77.5700, 13.0500));  // Yelahanka
        m.insert("560054", (77.5450, 13.0300));  // Gokula
        m.insert("560055", (77.5150, 13.0300));  // Rajgopal Nagar
        m.insert("560056", (77.5100, 12.9700));  // Nagarbhavi
        m.insert("560057", (77.5900, 12.8550));  // Sarakki
        m.insert("560058", (77.6550, 12.9050));  // Koramangala 6th Block
        m.insert("560059", (77.5250, 12.9200));  // Girinagar
        m.insert("560060", (77.5350, 12.8800));  // Kengeri Satellite Town
        m.insert("560061", (77.6450, 12.8800));  // Bommanahalli
        m.insert("560062", (77.5900, 13.0350));  // Bellary Road
        m.insert("560063", (77.6050, 12.8650));  // Arekere
        m.insert("560064", (77.5600, 13.0600));  // Sahakara Nagar
        m.insert("560065", (77.5600, 12.8600));  // Gottigere
        m.insert("560066", (77.6100, 13.0200));  // Kalyan Nagar
        m.insert("560067", (77.6400, 12.8550));  // Begur
        m.insert("560068", (77.6800, 12.9350));  // Domlur
        m.insert("560069", (77.5100, 12.9350));  // Mysore Road
        m.insert("560070", (77.6200, 12.8500));  // Bilekahalli
        m.insert("560071", (77.6900, 12.9550));  // Old Airport Road
        m.insert("560072", (77.5050, 13.0500));  // Dasarahalli
        m.insert("560073", (77.7100, 12.9700));  // Marathahalli
        m.insert("560074", (77.5800, 12.8400));  // Konanakunte
        m.insert("560075", (77.7000, 12.9350));  // Bellandur
        m.insert("560076", (77.5400, 12.8650));  // RR Nagar
        m.insert("560077", (77.6600, 12.8400));  // Hulimavu
        m.insert("560078", (77.6300, 13.0400));  // HBR Layout
        m.insert("560079", (77.6400, 13.0200));  // Thanisandra
        m.insert("560080", (77.5900, 12.8200));  // Kanakapura Road
        m.insert("560081", (77.5400, 12.8400));  // Rajarajeshwari Nagar
        m.insert("560082", (77.6750, 12.9150));  // Ejipura
        m.insert("560083", (77.6500, 13.0500));  // Jakkur
        m.insert("560084", (77.6550, 12.8650));  // Arakere Mico Layout
        m.insert("560085", (77.5100, 13.0700));  // Chikkabanavara
        m.insert("560086", (77.4950, 12.9300));  // Herohalli
        m.insert("560087", (77.5350, 12.8500));  // Channasandra
        m.insert("560088", (77.6850, 12.8700));  // Electronics City
        m.insert("560089", (77.7500, 12.8500));  // Sarjapur Road
        m.insert("560090", (77.6300, 12.8200));  // Gottigere South
        m.insert("560091", (77.5200, 13.0800));  // BEL Layout
        m.insert("560092", (77.6100, 12.8100));  // Vasanthapura
        m.insert("560093", (77.5500, 12.8300));  // Thalaghattapura
        m.insert("560094", (77.5650, 13.0700));  // Yelahanka New Town
        m.insert("560095", (77.5050, 12.8900));  // Kumbalgodu
        m.insert("560096", (77.5700, 13.0800));  // Allalasandra
        m.insert("560097", (77.7300, 12.9100));  // Sarjapur
        m.insert("560098", (77.7600, 12.9500));  // Varthur
        m.insert("560099", (77.6800, 13.0100));  // Ramamurthy Nagar
        m.insert("560100", (77.6700, 13.0400));  // Nagavara
        m.insert("560102", (77.5450, 13.0700));  // Kodigehalli
        m.insert("560103", (77.5650, 13.0900));  // Yelahanka Satellite Town
        m.insert("560104", (77.6500, 13.0700));  // Thanisandra Main Road
        m.insert("560105", (77.5900, 13.0600));  // Hebbal Kempapura
        m.insert("560107", (77.7400, 12.9800));  // Whitefield
        m.insert("560108", (77.5200, 13.0950));  // Bagalur
        m
    };
}

/// Returns (longitude, latitude) for a Bengaluru PIN code.
/// Falls back to Bangalore center if the PIN code is not found.
pub fn coordinates_for_pincode(pincode: &str) -> (f64, f64) {
    PINCODE_MAP.get(pincode).copied().unwrap_or(DEFAULT_COORDS)
}
