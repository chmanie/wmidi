enum MidiState {
    ExpectAnyByte,
    ExpectOneDataByte(u8),
    ExpectFirstDataByte(u8),
    ExpectSecondDataByte(u8, u8),
    ExpectOneRunningByte(u8),
    ExpectTwoRunningBytes(u8),
    ExpectSysExData(usize),
}

enum ByteType {
    StatusByteOne(u8),
    StatusByteTwo(u8),
    DataByte(u8),
    PassThroughByte(u8),
    SysExStartByte(u8),
    SysExStopByte(u8),
    UndefinedByte,
}

pub struct StreamParser<'a, const N: usize> {
    message_buffer: &'a mut [u8; N],
    state: MidiState,
}

// State machine loosely based on https://cdn.sparkfun.com/assets/learn_tutorials/4/0/8/midi-fsm3.png
impl<'a, const N: usize> StreamParser<'a, N> {
    pub fn new(message_buffer: &'a mut [u8; N]) -> Self {
        if N < 3 {
            panic!("Buffer needs to be at least 3 bytes long");
        }
        StreamParser {
            message_buffer,
            state: MidiState::ExpectAnyByte,
        }
    }
    pub fn push(&mut self, byte: u8) -> Option<&[u8]> {
        let byte = Self::get_byte_type(byte);
        match (&self.state, byte) {
            (MidiState::ExpectFirstDataByte(s), ByteType::DataByte(b)) => {
                self.state = MidiState::ExpectSecondDataByte(*s, b);
                return None;
            }
            (MidiState::ExpectSecondDataByte(s, b1), ByteType::DataByte(b2)) => {
                self.message_buffer[0] = *s;
                self.message_buffer[1] = *b1;
                self.message_buffer[2] = b2;
                self.state = MidiState::ExpectTwoRunningBytes(*s);
                return Some(&self.message_buffer[0..3]);
            }
            (MidiState::ExpectOneDataByte(s), ByteType::DataByte(b)) => {
                self.message_buffer[0] = *s;
                self.message_buffer[1] = b;
                self.state = MidiState::ExpectOneRunningByte(*s);
                return Some(&self.message_buffer[0..2]);
            }
            (MidiState::ExpectOneRunningByte(s), ByteType::DataByte(b)) => {
                self.message_buffer[0] = *s;
                self.message_buffer[1] = b;
                return Some(&self.message_buffer[0..2]);
            }
            (MidiState::ExpectTwoRunningBytes(s), ByteType::DataByte(b)) => {
                self.state = MidiState::ExpectSecondDataByte(*s, b);
                return None;
            }
            (MidiState::ExpectSysExData(n), ByteType::DataByte(b)) => {
                let n = *n + 1;
                self.message_buffer[n] = b;
                self.state = MidiState::ExpectSysExData(n);
                return None;
            }
            (MidiState::ExpectSysExData(n), ByteType::SysExStopByte(b)) => {
                let n = *n + 1;
                self.message_buffer[n] = b;
                self.state = MidiState::ExpectAnyByte;
                return Some(&self.message_buffer[0..n+1]);
            }
            (_, ByteType::StatusByteOne(s)) => {
                self.state = MidiState::ExpectOneDataByte(s);
                return None;
            }
            (_, ByteType::StatusByteTwo(s)) => {
                self.state = MidiState::ExpectFirstDataByte(s);
                return None;
            }
            (_, ByteType::SysExStartByte(s)) => {
                self.state = MidiState::ExpectSysExData(0);
                self.message_buffer[0] = s;
                return None;
            }
            (_, ByteType::PassThroughByte(b)) => {
                self.message_buffer[0] = b;
                return Some(&self.message_buffer[0..1]);
            }
            (_, ByteType::UndefinedByte) => None,
            _ => {
                self.state = MidiState::ExpectAnyByte;
                return None;
            }
        }
    }

    fn get_byte_type(byte: u8) -> ByteType {
        match byte & 0x80 {
            0x80 => match byte & 0xf0 {
                0xc | 0xd0 => ByteType::StatusByteOne(byte),
                0x80 | 0x90 | 0xa | 0xb | 0xe => ByteType::StatusByteTwo(byte),
                0xf0 => match byte {
                    0xf0 => ByteType::SysExStartByte(byte),
                    0xf1 => ByteType::StatusByteOne(byte),
                    0xf2 => ByteType::StatusByteTwo(byte),
                    0xf3 => ByteType::StatusByteOne(byte),
                    0xf7 => ByteType::SysExStopByte(byte),
                    0xf6 | 0xf8 | 0xf9 | 0xfa | 0xfb | 0xfc | 0xfd | 0xfe | 0xff => {
                        ByteType::PassThroughByte(byte)
                    }
                    _ => ByteType::UndefinedByte,
                },
                _ => ByteType::UndefinedByte,
            },
            _ => ByteType::DataByte(byte),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn note_on() {
        let mut message_buffer: [u8; 3] = [0; 3];
        let mut midi = StreamParser::new(&mut message_buffer);
        let bytes: &[u8] = &[0x90, 66, 44];
        assert_eq!(None, midi.push(bytes[0]));
        assert_eq!(None, midi.push(bytes[1]));
        assert_eq!(
            midi.push(bytes[2]).unwrap(),
            bytes,
            "Is NoteOn message",
        );
    }

    #[test]
    fn timing_clock() {
        let mut message_buffer: [u8; 3] = [0; 3];
        let mut midi = StreamParser::new(&mut message_buffer);
        let bytes: &[u8] = &[0xf8];
        assert_eq!(
            midi.push(bytes[0]).unwrap(),
            bytes,
            "Is Timing Clock message",
        );
    }

    #[test]
    fn sysex() {
        let mut message_buffer: [u8; 5] = [0; 5];
        let mut midi = StreamParser::new(&mut message_buffer);
        let bytes: &[u8] = &[0xf0, 99, 66, 33, 0xf7];
        assert_eq!(None, midi.push(bytes[0]));
        assert_eq!(None, midi.push(bytes[1]));
        assert_eq!(None, midi.push(bytes[2]));
        assert_eq!(None, midi.push(bytes[3]));
        assert_eq!(
            midi.push(bytes[4]).unwrap(),
            bytes,
            "Is SysEx message"
        );
    }

    #[test]
    fn real_time_messages() {
        let mut message_buffer: [u8; 3] = [0; 3];
        let mut midi = StreamParser::new(&mut message_buffer);
        let all_bytes: &[u8] = &[0x90, 0xf8, 66, 44];
        let real_time_byte: &[u8] = &all_bytes[1..2];
        let msg_bytes: &[u8] = &[all_bytes[0], all_bytes[2], all_bytes[3]];
        assert_eq!(None, midi.push(all_bytes[0]));
        assert_eq!(
            midi.push(all_bytes[1]).unwrap(),
            real_time_byte,
            "Real time message is extracted immediately"
        );
        assert_eq!(None, midi.push(all_bytes[2]));
        assert_eq!(
            midi.push(all_bytes[3]).unwrap(),
            msg_bytes,
            "Note On command is parsed properly"
        );
    }

    #[test]
    fn running_status_one_byte() {
        let mut message_buffer: [u8; 3] = [0; 3];
        let mut midi = StreamParser::new(&mut message_buffer);
        let all_bytes: &[u8] = &[0xd0, 66, 44];
        let msg_bytes: &[u8] = &all_bytes[0..2];
        let running_bytes: &[u8] = &[all_bytes[0], all_bytes[2]];
        assert_eq!(None, midi.push(all_bytes[0]));
        assert_eq!(
            midi.push(all_bytes[1]).unwrap(),
            msg_bytes,
            "Extract first message"
        );
        assert_eq!(
            midi.push(all_bytes[2]).unwrap(),
            running_bytes,
            "Extract running status message"
        );
    }

    #[test]
    fn running_status_two_bytes() {
        let mut message_buffer: [u8; 3] = [0; 3];
        let mut midi = StreamParser::new(&mut message_buffer);
        let all_bytes: &[u8] = &[0x90, 66, 44, 50, 22];
        let msg_bytes: &[u8] = &all_bytes[0..3];
        let running_bytes: &[u8] = &[all_bytes[0], all_bytes[3], all_bytes[4]];
        assert_eq!(None, midi.push(all_bytes[0]));
        assert_eq!(None, midi.push(all_bytes[1]));
        assert_eq!(
            midi.push(all_bytes[2]).unwrap(),
            msg_bytes,
            "Extract first message"
        );
        assert_eq!(None, midi.push(all_bytes[3]));
        assert_eq!(
            midi.push(all_bytes[4]).unwrap(),
            running_bytes,
            "Extract running status message"
        );
    }
}
