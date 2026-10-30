mod lcd;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{InterruptType, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::sys::EspError;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use std::thread;

use lcd::Lcd;

const NORMAL: u8 = 0;
const AJUSTAR_HOR: u8 = 1;
const AJUSTAR_MIN: u8 = 2;
const AJUSTAR_SEG: u8 = 3;

// Banderas atómicas para comunicar las interrupciones de los botones
static BOTON_1_PRESIONADO: AtomicBool = AtomicBool::new(false);
static BOTON_2_PRESIONADO: AtomicBool = AtomicBool::new(false);
static BOTON_3_PRESIONADO: AtomicBool = AtomicBool::new(false);
static BOTON_4_PRESIONADO: AtomicBool = AtomicBool::new(false);

// Variables atómicas compartidas entre la tarea de botones y la tarea del reloj/pantalla
static MODO: AtomicU8 = AtomicU8::new(0);
static HORA: AtomicU8 = AtomicU8::new(12);
static MINUTO: AtomicU8 = AtomicU8::new(30);
static SEGUNDO: AtomicU8 = AtomicU8::new(45);

static ACTUALIZAR_LCD: AtomicBool = AtomicBool::new(false);

fn main() -> Result<(), EspError> {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    let peripherals = Peripherals::take()?;

    let rs = PinDriver::output(peripherals.pins.gpio15)?;
    let en = PinDriver::output(peripherals.pins.gpio16)?;
    let d4 = PinDriver::output(peripherals.pins.gpio17)?;
    let d5 = PinDriver::output(peripherals.pins.gpio18)?;
    let d6 = PinDriver::output(peripherals.pins.gpio8)?;
    let d7 = PinDriver::output(peripherals.pins.gpio3)?;

    // Configuración de los botones con pull-up e interrupción por flanco de bajada
    let mut boton1 = PinDriver::input(peripherals.pins.gpio4, Pull::Up)?;
    let mut boton2 = PinDriver::input(peripherals.pins.gpio5, Pull::Up)?;
    let mut boton3 = PinDriver::input(peripherals.pins.gpio6, Pull::Up)?;
    let mut boton4 = PinDriver::input(peripherals.pins.gpio7, Pull::Up)?;

    boton1.set_interrupt_type(InterruptType::NegEdge)?;
    boton2.set_interrupt_type(InterruptType::NegEdge)?;
    boton3.set_interrupt_type(InterruptType::NegEdge)?;
    boton4.set_interrupt_type(InterruptType::NegEdge)?;

    unsafe {
        boton1.subscribe(|| {
            BOTON_1_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
        boton2.subscribe(|| {
            BOTON_2_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
        boton3.subscribe(|| {
            BOTON_3_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
        boton4.subscribe(|| {
            BOTON_4_PRESIONADO.store(true, Ordering::Relaxed);
        })?;
    }

    boton1.enable_interrupt()?;
    boton2.enable_interrupt()?;
    boton3.enable_interrupt()?;
    boton4.enable_interrupt()?;

    let mut lcd = Lcd::new(rs, en, d4, d5, d6, d7)?;
    lcd.clear()?;
    log::info!("LCD inicializada correctamente.");

    thread::Builder::new()
        .name("tarea_lcd".to_string())
        .stack_size(4096)
        .spawn(move || {
            log::info!("Tarea de escritura en la LCD inicializada correctamente.");
            loop {
                if ACTUALIZAR_LCD.swap(false, Ordering::Relaxed) {
                    let h = HORA.load(Ordering::Relaxed);
                    let m = MINUTO.load(Ordering::Relaxed);
                    let s = SEGUNDO.load(Ordering::Relaxed);

                    lcd.set_cursor(0, 0).unwrap();
                    let tiempo = format!("Hora:   {:02}:{:02}:{:02}", h, m, s);
                    write!(&mut lcd, "{}", tiempo).unwrap();

                    lcd.set_cursor(0, 1).unwrap();
                    let m = MODO.load(Ordering::Relaxed);

                    match m {
                        NORMAL => {
                            write!(&mut lcd, "Modo:   NORMAL").unwrap();
                        }
                        AJUSTAR_HOR => {
                            write!(&mut lcd, "Modo:   HOR   ").unwrap();
                        }
                        AJUSTAR_MIN => {
                            write!(&mut lcd, "Modo:   MIN   ").unwrap();
                        }
                        _ => {
                            write!(&mut lcd, "Modo:   SEG   ").unwrap();
                        }
                    }
                }

                FreeRtos::delay_ms(100);
            }
        })
        .expect("Error al iniciar la tarea de la LCD");

    //TAERA PARA ACTUALIZAR LA HORA
    thread::Builder::new()
        .name("tarea_reloj".to_string())
        .stack_size(4096)
        .spawn(move || loop {
            FreeRtos::delay_ms(1000);

            let mut h = HORA.load(Ordering::Relaxed);
            let mut m = MINUTO.load(Ordering::Relaxed);
            let mut s = SEGUNDO.load(Ordering::Relaxed);

            s += 1;
            if s == 60 {
                s = 0;
                m += 1;
                if m == 60 {
                    m = 0;
                    h += 1;
                    if h == 24 {
                        h = 0;
                    }
                }
            }

            HORA.store(h, Ordering::Relaxed);
            MINUTO.store(m, Ordering::Relaxed);
            SEGUNDO.store(s, Ordering::Relaxed);

            log::info!("{:02}:{:02}:{:02}", h, m, s);
            ACTUALIZAR_LCD.store(true, Ordering::Relaxed);
        })
        .expect("Error al iniciar la tarea del reloj");

    loop {
        FreeRtos::delay_ms(50);

        if BOTON_1_PRESIONADO.swap(false, Ordering::Relaxed) {
            let mut m = MODO.load(Ordering::Relaxed);

            m = (m + 1) % 4;

            MODO.store(m, Ordering::Relaxed);

            FreeRtos::delay_ms(150);
            boton1.enable_interrupt()?;
            ACTUALIZAR_LCD.store(true, Ordering::Relaxed);
        }

        if BOTON_2_PRESIONADO.swap(false, Ordering::Relaxed) {
            let m = MODO.load(Ordering::Relaxed);
            match m {
                AJUSTAR_HOR => {
                    let mut h = HORA.load(Ordering::Relaxed);
                    h = (h + 1) % 24;
                    HORA.store(h, Ordering::Relaxed);
                }
                AJUSTAR_MIN => {
                    let mut m = MINUTO.load(Ordering::Relaxed);
                    m = (m + 1) % 60;
                    MINUTO.store(m, Ordering::Relaxed);
                }
                AJUSTAR_SEG => {
                    let mut s = SEGUNDO.load(Ordering::Relaxed);
                    s = (s + 1) % 60;
                    SEGUNDO.store(s, Ordering::Relaxed);
                }
                _ => {}
            }
            FreeRtos::delay_ms(150);
            boton2.enable_interrupt()?;
            ACTUALIZAR_LCD.store(true, Ordering::Relaxed);
        }

        if BOTON_3_PRESIONADO.swap(false, Ordering::Relaxed) {
            let m = MODO.load(Ordering::Relaxed);
            match m {
                AJUSTAR_HOR => {
                    let mut h = HORA.load(Ordering::Relaxed);
                    h = (h + 23) % 24;
                    HORA.store(h, Ordering::Relaxed);
                }
                AJUSTAR_MIN => {
                    let mut m = MINUTO.load(Ordering::Relaxed);
                    m = (m + 59) % 60;
                    MINUTO.store(m, Ordering::Relaxed);
                }
                AJUSTAR_SEG => {
                    let mut s = SEGUNDO.load(Ordering::Relaxed);
                    s = (s + 59) % 60;
                    SEGUNDO.store(s, Ordering::Relaxed);
                }
                _ => {}
            }
            FreeRtos::delay_ms(150);
            boton3.enable_interrupt()?;
            ACTUALIZAR_LCD.store(true, Ordering::Relaxed);
        }

        if BOTON_4_PRESIONADO.swap(false, Ordering::Relaxed) {
            MODO.store(NORMAL, Ordering::Relaxed);
            FreeRtos::delay_ms(150);
            boton4.enable_interrupt()?;
            ACTUALIZAR_LCD.store(true, Ordering::Relaxed);
        }
    }
}
