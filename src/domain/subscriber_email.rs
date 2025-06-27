use validator::validate_email;

#[derive(Debug)]
pub struct SubscriberEmail(String);

impl SubscriberEmail {
    pub fn parse(s: String) -> Result<SubscriberEmail, String> {
        if validate_email(&s) {
            Ok(Self(s))
        } else {
            Err(format!("'{s}' is not a valid subscriber email."))
        }
    }
}

impl std::fmt::Display for SubscriberEmail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for SubscriberEmail {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriberEmail;
    use claim::assert_err;
    use fake::{Fake, faker::internet::en::SafeEmail};
    use rand::{SeedableRng, rngs::StdRng};

    #[derive(Debug, Clone)]
    struct ValidEmailFixture(pub String);

    // How quickcheck should generate the test data
    impl quickcheck::Arbitrary for ValidEmailFixture {
        fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Self {
            let seed = u64::arbitrary(g);
            //Create a seeded RNG that fake can use
            let mut rng = StdRng::seed_from_u64(seed);
            let email = SafeEmail().fake_with_rng(&mut rng);

            Self(email)
        }
    }

    #[test]
    fn empty_string_is_rejected() {
        let email = String::from("");
        assert_err!(SubscriberEmail::parse(email));
    }

    #[test]
    fn email_missing_at_symbol_is_rejected() {
        let email = "ursuladomain.com".to_string();
        assert_err!(SubscriberEmail::parse(email));
    }

    #[test]
    fn email_missing_subject_is_rejected() {
        let email = String::from("@domail.com");
        assert_err!(SubscriberEmail::parse(email));
    }

    // Calls function default 100 times and if error is met narrows it down
    #[quickcheck_macros::quickcheck]
    fn valid_emails_are_parsed_successfully(valid_email: ValidEmailFixture) -> bool {
        SubscriberEmail::parse(valid_email.0).is_ok()
    }
}
