//! Every replicated topic has exactly one primitive SDK reading.
use omega::internal::Wiring;
use omega_proto::SystemTopic;
struct Coverage;
impl Coverage {
    const TOPICS: &'static [&'static [SystemTopic]] = &[
        <omega::platform::applications::Applications as Wiring>::TOPICS,
        <omega::platform::power::Battery as Wiring>::TOPICS,
        <omega::platform::power::Mains as Wiring>::TOPICS,
        <omega::platform::power::PowerProfiles as Wiring>::TOPICS,
        <omega::platform::power::Peripherals as Wiring>::TOPICS,
        <omega::platform::network::Network as Wiring>::TOPICS,
        <omega::platform::network::Wifi as Wiring>::TOPICS,
        <omega::platform::network::Vpn as Wiring>::TOPICS,
        <omega::platform::network::Throughput as Wiring>::TOPICS,
        <omega::platform::bluetooth::Bluetooth as Wiring>::TOPICS,
        <omega::platform::audio::Audio as Wiring>::TOPICS,
        <omega::platform::audio::Media as Wiring>::TOPICS,
        <omega::platform::desktop::Backlight as Wiring>::TOPICS,
        <omega::platform::desktop::Monitors as Wiring>::TOPICS,
        <omega::platform::desktop::Workspaces as Wiring>::TOPICS,
        <omega::platform::desktop::Window as Wiring>::TOPICS,
        <omega::platform::desktop::Input as Wiring>::TOPICS,
        <omega::platform::time::Clock as Wiring>::TOPICS,
        <omega::platform::session::Idle as Wiring>::TOPICS,
        <omega::platform::system::System as Wiring>::TOPICS,
        <omega::platform::system::Disk as Wiring>::TOPICS,
        <omega::platform::system::Thermals as Wiring>::TOPICS,
        <omega::plugin::Units as Wiring>::TOPICS,
    ];
}
#[test]
fn every_topic_has_one_primitive_reading() {
    let mut actual: Vec<_> = Coverage::TOPICS
        .iter()
        .flat_map(|topics| topics.iter().copied())
        .collect();
    actual.sort();
    let mut expected = SystemTopic::ALL.to_vec();
    expected.sort();
    assert_eq!(actual, expected);
}
