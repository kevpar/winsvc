#[cfg(test)]
mod test {
    use std::error::Error;
    use thiserror::Error;

    #[derive(Debug)]
    struct MyErr(Option<Box<dyn Error + 'static>>);

    impl MyErr {
        // fn new() -> Self {
        //     MyErr(None)
        // }

        fn wrap(err: Box<dyn Error + 'static>) -> Self {
            MyErr(Some(err))
        }
    }

    impl std::fmt::Display for MyErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MyErr")
        }
    }

    impl std::error::Error for MyErr {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self.0.as_ref()?.as_ref())
        }
    }

    #[derive(Debug)]
    enum EnumErr {
        E1,
        E2,
    }

    impl std::fmt::Display for EnumErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::E1 => write!(f, "E1"),
                Self::E2 => write!(f, "E2"),
            }
        }
    }

    impl std::error::Error for EnumErr {}

    #[derive(Error, Debug)]
    enum SmartErr {
        #[error("something")]
        Something,
    }

    #[test]
    fn foo() {
        let e = windows_service::Error::LaunchArgumentsNotSupported;
        let e2: Box<dyn std::error::Error> = Box::new(MyErr::wrap(Box::new(
            windows_service::Error::LaunchArgumentsNotSupported,
        )));
        let v = EnumErr::E1;
        let v2 = EnumErr::E2;
        let es = e2.source().unwrap();
        let s = SmartErr::Something;
        let a = anyhow::Error::new(windows_service::Error::LaunchArgumentsNotSupported);
        let a2 =
            anyhow::Error::new(windows_service::Error::LaunchArgumentsNotSupported).context("foo");
        // let e = e.with_context
        eprintln!("1: {} {:?}", e, e);
        eprintln!("2: {} {:?}", e2, e2);
        eprintln!("3: {} {:?}", v, v);
        eprintln!("4: {} {:?}", v2, v2);
        eprintln!("5: {} {:?}", es, es);
        eprintln!("6: {} {:#} {:?}", s, s, s);
        eprintln!("7: {} {:#} {:?}", a, a, a);
        eprintln!("8: {} {:#} {:?}", a2, a2, a2);
    }
}
