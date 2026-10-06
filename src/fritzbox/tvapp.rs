// SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Channel logos used by the FRITZ!Box web interface, located at
//! <https://tv.avm.de/tvapp/logos/>.
//!
//! There is no directory listing, so the file names are derived from the
//! channel names: lowercase, umlauts transcribed, `.`, `-` and `,` removed,
//! spaces, `/` and `+` replaced by `_`. HD logos are in `hd/`, radio logos
//! in `radio/`. Some channels don't follow this scheme and are listed below.

use super::ChannelList;

pub const TVAPP_LOGOS_URL: &str = "https://tv.avm.de/tvapp/logos/";

/// File names in the top-level directory that don't follow the scheme.
const SD_ALIASES: &[(&str, &str)] = &[
    ("123tv", "einszweidrei_tv"),
    ("anixe", "anixe_sd"),
    ("kabel_eins", "kabel1"),
    ("pro7_maxx", "pro7maxx"),
    ("prosieben", "pro7"),
    ("rtlzwei", "rtl2"),
    ("sat1_gold", "sat1gold"),
    ("sonnenklartv", "sonnenklar"),
    ("super_rtl", "superrtl"),
    ("tele_5", "tele5"),
];

/// File names in `hd/` that don't follow the scheme.
const HD_ALIASES: &[(&str, &str)] = &[
    ("hrfernsehen_hd", "hr_hd"),
    ("sr_fernsehen_hd", "sr_hd"),
    ("zdfinfo_hd", "zdf_info_hd"),
];

/// Regional variants of HD channels that share one logo in `hd/`.
const HD_REGIONAL_PREFIXES: &[(&str, &str)] = &[
    ("br_fernsehen_", "br_hd"),
    ("mdr_", "mdr_hd"),
    ("ndr_fs_", "ndr_hd"),
    ("rbb_", "rbb_hd"),
    ("wdr_hd_", "wdr_hd"),
];

/// File names in `radio/` that don't follow the scheme.
const RADIO_ALIASES: &[(&str, &str)] = &[("ndr_903", "ndr903")];

/// Returns the logo paths relative to [`TVAPP_LOGOS_URL`] that may exist for
/// `channel_name`, most likely first.
///
/// The paths never contain `..` and are safe to use as relative file paths.
pub fn logo_paths(channel_name: &str, list: ChannelList) -> Vec<String> {
    let name = file_name(channel_name);
    if name.is_empty() {
        return Vec::new();
    }

    let sd = || format!("{}.png", alias(SD_ALIASES, &name).unwrap_or(&name));
    let hd = || {
        let file_name = alias(HD_ALIASES, &name)
            .or_else(|| {
                HD_REGIONAL_PREFIXES
                    .iter()
                    .find(|(prefix, _)| name.starts_with(prefix))
                    .map(|(_, file_name)| *file_name)
            })
            .unwrap_or(&name);
        format!("hd/{file_name}.png")
    };

    match list {
        ChannelList::Radio => vec![format!(
            "radio/{}.png",
            alias(RADIO_ALIASES, &name).unwrap_or(&name)
        )],
        ChannelList::Hd => vec![hd(), sd()],
        ChannelList::Sd => vec![sd(), hd()],
        // the combined list doesn't tell which channels are HD
        ChannelList::Tv if name.split('_').any(|word| word == "hd") => vec![hd(), sd()],
        ChannelList::Tv => vec![sd(), hd()],
    }
}

/// Derives the file name (without extension) from a channel name, e.g.
/// `br_fernsehen_sued_hd` from `BR Fernsehen Süd HD`.
fn file_name(channel_name: &str) -> String {
    let mut file_name = String::new();

    for c in channel_name.chars().flat_map(char::to_lowercase) {
        match c {
            'a'..='z' | '0'..='9' | '*' | '(' | ')' => file_name.push(c),
            'ä' => file_name.push_str("ae"),
            'ö' => file_name.push_str("oe"),
            'ü' => file_name.push_str("ue"),
            'ß' => file_name.push_str("ss"),
            '_' | '/' | '+' => push_separator(&mut file_name),
            c if c.is_whitespace() => push_separator(&mut file_name),
            // `.`, `-`, `,` and anything unknown
            _ => (),
        }
    }

    file_name.truncate(file_name.trim_end_matches('_').len());
    file_name
}

fn push_separator(file_name: &mut String) {
    if !file_name.is_empty() && !file_name.ends_with('_') {
        file_name.push('_');
    }
}

fn alias<'a>(aliases: &[(&str, &'a str)], name: &str) -> Option<&'a str> {
    aliases
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, file_name)| *file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Channels from the combined TV list and the logos shown for them by the
    /// FRITZ!Box web interface.
    const TV: &[(&str, &str)] = &[
        ("Sat.1", "sat1.png"),
        ("ProSieben", "pro7.png"),
        ("ntv", "ntv.png"),
        ("CNN", "cnn.png"),
        ("DMAX", "dmax.png"),
        ("SPORT1", "sport1.png"),
        ("Kabel Eins", "kabel1.png"),
        ("VOX", "vox.png"),
        ("sixx", "sixx.png"),
        ("RTLZWEI", "rtl2.png"),
        ("TELE 5", "tele5.png"),
        ("Disney Channel", "disney_channel.png"),
        ("RTLNITRO", "rtlnitro.png"),
        ("SAT.1 Gold", "sat1gold.png"),
        ("Pro7 MAXX", "pro7maxx.png"),
        ("SUPER RTL", "superrtl.png"),
        ("Bibel TV", "bibel_tv.png"),
        ("QVC", "qvc.png"),
        ("1-2-3.tv", "einszweidrei_tv.png"),
        ("TLC", "tlc.png"),
        ("Channel21", "channel21.png"),
        ("sonnenklar.TV", "sonnenklar.png"),
        ("ANIXE+", "anixe_sd.png"),
        ("DMF", "dmf.png"),
        ("Sonlife", "sonlife.png"),
        ("Das Erste HD", "hd/das_erste_hd.png"),
        ("ZDF HD", "hd/zdf_hd.png"),
        ("KiKA HD", "hd/kika_hd.png"),
        ("arte HD", "hd/arte_hd.png"),
        ("3sat HD", "hd/3sat_hd.png"),
        ("WDR HD Köln", "hd/wdr_hd.png"),
        ("BR Fernsehen Süd HD", "hd/br_hd.png"),
        ("hr-fernsehen HD", "hd/hr_hd.png"),
        ("NDR FS HH HD", "hd/ndr_hd.png"),
        ("NDR FS MV HD", "hd/ndr_hd.png"),
        ("NDR FS NDS HD", "hd/ndr_hd.png"),
        ("NDR FS SH HD", "hd/ndr_hd.png"),
        ("rbb Berlin HD", "hd/rbb_hd.png"),
        ("rbb Brandenburg HD", "hd/rbb_hd.png"),
        ("MDR Sachsen HD", "hd/mdr_hd.png"),
        ("phoenix HD", "hd/phoenix_hd.png"),
        ("zdf_neo HD", "hd/zdf_neo_hd.png"),
        ("QVC HD", "hd/qvc_hd.png"),
        ("Pyur Infokanal HD", "hd/pyur_infokanal_hd.png"),
        ("8Sport HD", "hd/8sport_hd.png"),
        ("TOGGO Plus", "toggo_plus.png"),
        ("BBC News", "bbc_news.png"),
        ("ARD alpha HD", "hd/ard_alpha_hd.png"),
        (
            "Al Jazeera Satellite Channel HD",
            "hd/al_jazeera_satellite_channel_hd.png",
        ),
        ("Handystar TV", "handystar_tv.png"),
        ("N24 DOKU", "n24_doku.png"),
        ("1-2-3.tv HD", "hd/123tv_hd.png"),
        ("Bloomberg Europe TV", "bloomberg_europe_tv.png"),
        ("HGTV", "hgtv.png"),
        ("Shop LC", "shop_lc.png"),
        ("Spreekanal", "spreekanal.png"),
        ("QVC ZWEI HD", "hd/qvc_zwei_hd.png"),
        ("RTL Television", "rtl_television.png"),
        ("DF 1 HD", "hd/df_1_hd.png"),
        ("RTLup", "rtlup.png"),
        ("teltOwkanal HD", "hd/teltowkanal_hd.png"),
        ("Schlager Deluxe", "schlager_deluxe.png"),
        ("DOKUSAT", "dokusat.png"),
        ("HSE EXTRA SD", "hse_extra_sd.png"),
        ("HSE HD", "hd/hse_hd.png"),
        ("Service 11000", "service_11000.png"),
        ("Channel21 HD", "hd/channel21_hd.png"),
        ("EspresoTV HD", "hd/espresotv_hd.png"),
        ("VOXup SD", "voxup_sd.png"),
        ("EURONEWS GERMAN SD", "euronews_german_sd.png"),
        ("NICK/CC+1", "nick_cc_1.png"),
        ("HSE", "hse.png"),
        ("tagesschau24 HD", "hd/tagesschau24_hd.png"),
        ("MTV", "mtv.png"),
        ("HSE Trend", "hse_trend.png"),
        ("WELT", "welt.png"),
        ("DELUXE MUSIC", "deluxe_music.png"),
        ("Comedy Central", "comedy_central.png"),
        ("Eurosport 1 Deutschland", "eurosport_1_deutschland.png"),
        ("Melodie TV", "melodie_tv.png"),
        ("ShopLC HD", "hd/shoplc_hd.png"),
        ("SWR BW HD", "hd/swr_bw_hd.png"),
        ("kw-tv HD", "hd/kwtv_hd.png"),
        ("Saudi TV HD", "hd/saudi_tv_hd.png"),
        ("Service 65534", "service_65534.png"),
        ("ALEX BERLIN HD", "hd/alex_berlin_hd.png"),
        ("QVC STYLE HD", "hd/qvc_style_hd.png"),
        ("ONE HD", "hd/one_hd.png"),
        ("Lilo.TV", "lilotv.png"),
        ("TELEGOLD", "telegold.png"),
        ("Kabel Eins Doku", "kabel_eins_doku.png"),
        ("8Sport", "8sport.png"),
        ("sonnenklar.TV HD", "hd/sonnenklartv_hd.png"),
        ("volksmusik.TV", "volksmusiktv.png"),
        ("FREEDOM", "freedom.png"),
        ("SR Fernsehen HD", "hd/sr_hd.png"),
        ("HSE EXTRA HD", "hd/hse_extra_hd.png"),
        ("nice Berlin", "nice_berlin.png"),
        ("TV BERLIN", "tv_berlin.png"),
        ("Radio Bremen HD", "hd/radio_bremen_hd.png"),
        ("ZDFinfo HD", "hd/zdf_info_hd.png"),
        ("Bibel TV HD", "hd/bibel_tv_hd.png"),
    ];

    /// HD channels from the combined TV list without “HD” in their name.
    const TV_HD_WITHOUT_SUFFIX: &[(&str, &str)] = &[
        ("TV5MONDE", "hd/tv5monde.png"),
        (
            "WDR Mediathek (Internet)",
            "hd/wdr_mediathek_(internet).png",
        ),
        ("ARD-Test-R", "hd/ardtestr.png"),
        ("ARD-Test-1", "hd/ardtest1.png"),
    ];

    /// Channels from the radio list and the logos shown for them by the
    /// FRITZ!Box web interface.
    const RADIO: &[(&str, &str)] = &[
        ("Star FM", "radio/star_fm.png"),
        ("BR24live", "radio/br24live.png"),
        ("JAM FM", "radio/jam_fm.png"),
        ("Spreeradio", "radio/spreeradio.png"),
        ("BB Radio", "radio/bb_radio.png"),
        ("WDR 5", "radio/wdr_5.png"),
        ("ANTENNE BAYERN", "radio/antenne_bayern.png"),
        ("BAYERN 3", "radio/bayern_3.png"),
        ("rbb24 Inforadio", "radio/rbb24_inforadio.png"),
        ("MDR SPUTNIK", "radio/mdr_sputnik.png"),
        ("radioeins", "radio/radioeins.png"),
        ("LAUSITZWELLE", "radio/lausitzwelle.png"),
        ("Antenne Brbg.", "radio/antenne_brbg.png"),
        ("Dlf Nova", "radio/dlf_nova.png"),
        ("SWR3", "radio/swr3.png"),
        ("MDR KLASSIK", "radio/mdr_klassik.png"),
        ("WDR 3", "radio/wdr_3.png"),
        ("Berliner Rundfunk 91.4", "radio/berliner_rundfunk_914.png"),
        ("SWR Aktuell", "radio/swr_aktuell.png"),
        ("NDR 1 Nieders. HAN", "radio/ndr_1_nieders_han.png"),
        ("SWR4 BW", "radio/swr4_bw.png"),
        ("Dlf", "radio/dlf.png"),
        ("NDR 90,3", "radio/ndr903.png"),
        ("MDR SACHSEN DD", "radio/mdr_sachsen_dd.png"),
        ("100.6 Flux FM", "radio/1006_flux_fm.png"),
        ("BR-KLASSIK", "radio/brklassik.png"),
        ("DASDING", "radio/dasding.png"),
        ("Antenne Niedersachsen", "radio/antenne_niedersachsen.png"),
        ("BR-Heimat", "radio/brheimat.png"),
        ("ROCK ANTENNE", "radio/rock_antenne.png"),
        ("Bremen Eins", "radio/bremen_eins.png"),
        ("BR24", "radio/br24.png"),
        ("SWR Kultur", "radio/swr_kultur.png"),
        ("Radio Bob", "radio/radio_bob.png"),
        ("Schlagerradio", "radio/schlagerradio.png"),
        ("Antenne Schlager", "radio/antenne_schlager.png"),
        ("1LIVE", "radio/1live.png"),
        ("STAR*SAT RADIO", "radio/star*sat_radio.png"),
        ("COSMO", "radio/cosmo.png"),
        ("1LIVE diGGi", "radio/1live_diggi.png"),
        ("sunshine live", "radio/sunshine_live.png"),
        ("MEGARADIOmix", "radio/megaradiomix.png"),
        ("Ohrsicht Radio", "radio/ohrsicht_radio.png"),
        ("CHILL IN THE MIX", "radio/chill_in_the_mix.png"),
        ("Rockland", "radio/rockland.png"),
        ("Klassik Radio", "radio/klassik_radio.png"),
        ("Radio Cottbus", "radio/radio_cottbus.png"),
        ("Radio Paradiso", "radio/radio_paradiso.png"),
        ("Radio Teddy", "radio/radio_teddy.png"),
        ("98.8 Kiss FM", "radio/988_kiss_fm.png"),
        ("WDR 2", "radio/wdr_2.png"),
        ("Bremen Vier", "radio/bremen_vier.png"),
        ("Fritz", "radio/fritz.png"),
        ("WDR 4", "radio/wdr_4.png"),
        ("hr1", "radio/hr1.png"),
        ("hr INFO", "radio/hr_info.png"),
        ("Big FM", "radio/big_fm.png"),
        ("94.3 rs2", "radio/943_rs2.png"),
        ("BR Schlager", "radio/br_schlager.png"),
        ("MDR S-ANHALT MD", "radio/mdr_sanhalt_md.png"),
        ("MDR AKTUELL", "radio/mdr_aktuell.png"),
        ("SWR4 RP", "radio/swr4_rp.png"),
        ("OLDIE ANTENNE", "radio/oldie_antenne.png"),
        ("MDR THÜR Mitte-W", "radio/mdr_thuer_mittew.png"),
        ("detektor.fm", "radio/detektorfm.png"),
        ("Radio Potsdam", "radio/radio_potsdam.png"),
        ("RTL RADIO", "radio/rtl_radio.png"),
        ("Radio Paloma", "radio/radio_paloma.png"),
        ("ERF Plus", "radio/erf_plus.png"),
        ("SR kultur", "radio/sr_kultur.png"),
        ("MDR JUMP", "radio/mdr_jump.png"),
        ("Mega 80s", "radio/mega_80s.png"),
        ("egoFM", "radio/egofm.png"),
        ("NDR Kultur", "radio/ndr_kultur.png"),
        ("Dlf Kultur", "radio/dlf_kultur.png"),
        ("NDR1 Radio MV SN", "radio/ndr1_radio_mv_sn.png"),
        ("NDR1 Welle Nord KI", "radio/ndr1_welle_nord_ki.png"),
        ("hr4", "radio/hr4.png"),
        ("Bayern 1", "radio/bayern_1.png"),
        ("NDR Schlager", "radio/ndr_schlager.png"),
        ("YOU FM", "radio/you_fm.png"),
        ("NDR 2 NDS", "radio/ndr_2_nds.png"),
        ("SWR 5.1 Kultur", "radio/swr_51_kultur.png"),
        ("N-JOY", "radio/njoy.png"),
        ("Bayern 2", "radio/bayern_2.png"),
        ("radio3", "radio/radio3.png"),
        ("SWR1 RP", "radio/swr1_rp.png"),
        ("NDR Info NDS", "radio/ndr_info_nds.png"),
        ("MDR KULTUR", "radio/mdr_kultur.png"),
        ("Bremen NEXT", "radio/bremen_next.png"),
        ("NDR Blue", "radio/ndr_blue.png"),
        ("WDR Event", "radio/wdr_event.png"),
        ("Die Maus", "radio/die_maus.png"),
        ("SR 3 Saaarlandwelle", "radio/sr_3_saaarlandwelle.png"),
        ("MEGA 90s", "radio/mega_90s.png"),
        ("hr2", "radio/hr2.png"),
        ("104.6 RTL", "radio/1046_rtl.png"),
        ("hr3", "radio/hr3.png"),
        ("NDR Info Spezial", "radio/ndr_info_spezial.png"),
        ("DRadio DokDeb", "radio/dradio_dokdeb.png"),
        ("SWR1 BW", "radio/swr1_bw.png"),
        ("SR1 Europawelle", "radio/sr1_europawelle.png"),
        ("rbb 88.8", "radio/rbb_888.png"),
        ("Energy Berlin", "radio/energy_berlin.png"),
        ("Bremen Zwei", "radio/bremen_zwei.png"),
    ];

    #[test]
    fn matches_web_interface_tv_logos() {
        for (name, path) in TV {
            assert_eq!(logo_paths(name, ChannelList::Tv)[0], *path, "{name}");
        }
        for (name, path) in TV_HD_WITHOUT_SUFFIX {
            assert_eq!(logo_paths(name, ChannelList::Tv)[1], *path, "{name}");
        }
    }

    #[test]
    fn matches_web_interface_radio_logos() {
        for (name, path) in RADIO {
            assert_eq!(logo_paths(name, ChannelList::Radio), [*path], "{name}");
        }
    }

    #[test]
    fn prefers_the_directory_of_the_list() {
        assert_eq!(
            logo_paths("TV5MONDE", ChannelList::Hd),
            ["hd/tv5monde.png", "tv5monde.png"]
        );
        assert_eq!(
            logo_paths("Das Erste HD", ChannelList::Sd),
            ["das_erste_hd.png", "hd/das_erste_hd.png"]
        );
        assert_eq!(
            logo_paths("Kabel Eins", ChannelList::Sd),
            ["kabel1.png", "hd/kabel_eins.png"]
        );
    }

    #[test]
    fn derives_safe_file_names() {
        assert_eq!(file_name("  ../etc/passwd "), "etc_passwd");
        assert_eq!(file_name("A  -  B"), "a_b");
        assert_eq!(file_name("Télé Ω"), "tl");
        assert!(logo_paths("", ChannelList::Tv).is_empty());
        assert!(logo_paths("!!!", ChannelList::Radio).is_empty());

        for (name, _) in TV.iter().chain(TV_HD_WITHOUT_SUFFIX).chain(RADIO) {
            for list in [ChannelList::Tv, ChannelList::Hd, ChannelList::Sd] {
                for path in logo_paths(name, list) {
                    assert!(!path.contains(".."), "{path}");
                    assert!(!path.starts_with('/'), "{path}");
                }
            }
        }
    }
}
